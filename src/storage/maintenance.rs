//! Pure advisory selection and cursor codec. No filesystem effects, admission,
//! provenance proofs, retirement API or deletion authority live here.
use sha2::{Digest, Sha256};

const CATEGORIES: usize = 8;
const ADDRESS_BYTES: usize = 64;
const BODY_BYTES: usize = 8 + 32 + 1 + CATEGORIES * (1 + ADDRESS_BYTES);
pub(crate) const CURSOR_BYTES: usize = BODY_BYTES + 32;
const MAGIC: &[u8; 8] = b"CGMC0001";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WorkCategory {
    Canonical,
    TemporaryOrBackup,
    Run,
    Phase,
    Generation,
    LegacyNative,
    LegacySnapshot,
    DefaultDirect,
}
impl WorkCategory {
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Canonical,
            1 => Self::TemporaryOrBackup,
            2 => Self::Run,
            3 => Self::Phase,
            4 => Self::Generation,
            5 => Self::LegacyNative,
            6 => Self::LegacySnapshot,
            7 => Self::DefaultDirect,
            _ => unreachable!("validated category"),
        }
    }
}

/// Caller supplies a lossless fixed-width logical address, not a truncated or
/// hashed path. This module neither resolves it nor certifies its association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CursorAddress(pub(crate) [u8; ADDRESS_BYTES]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorFailure {
    Corrupt,
    DifferentInstallation,
}

pub(crate) struct MaintenanceCursor {
    installation: [u8; 32],
    next_category: u8,
    after: [Option<CursorAddress>; CATEGORIES],
}
impl MaintenanceCursor {
    pub(crate) fn new(installation: [u8; 32]) -> Self {
        Self {
            installation,
            next_category: 0,
            after: [None; CATEGORIES],
        }
    }
    pub(crate) fn encode(&self) -> [u8; CURSOR_BYTES] {
        let mut bytes = [0u8; CURSOR_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..40].copy_from_slice(&self.installation);
        bytes[40] = self.next_category;
        for (index, address) in self.after.iter().enumerate() {
            if let Some(address) = address {
                let offset = 41 + index * 65;
                bytes[offset] = 1;
                bytes[offset + 1..offset + 65].copy_from_slice(&address.0);
            }
        }
        let hash = Sha256::digest(&bytes[..BODY_BYTES]);
        bytes[BODY_BYTES..].copy_from_slice(&hash);
        bytes
    }
    pub(crate) fn decode(
        bytes: &[u8; CURSOR_BYTES],
        installation: [u8; 32],
    ) -> Result<Self, CursorFailure> {
        if &bytes[..8] != MAGIC
            || bytes[40] as usize >= CATEGORIES
            || bytes[BODY_BYTES..] != Sha256::digest(&bytes[..BODY_BYTES])[..]
        {
            return Err(CursorFailure::Corrupt);
        }
        if bytes[8..40] != installation {
            return Err(CursorFailure::DifferentInstallation);
        }
        let mut result = Self::new(installation);
        result.next_category = bytes[40];
        for index in 0..CATEGORIES {
            let offset = 41 + index * 65;
            match bytes[offset] {
                0 if bytes[offset + 1..offset + 65].iter().all(|byte| *byte == 0) => {}
                1 => {
                    let mut address = [0u8; ADDRESS_BYTES];
                    address.copy_from_slice(&bytes[offset + 1..offset + 65]);
                    result.after[index] = Some(CursorAddress(address));
                }
                _ => return Err(CursorFailure::Corrupt),
            }
        }
        Ok(result)
    }
    pub(crate) fn selection(&self) -> FairSelection {
        FairSelection {
            next_category: self.next_category,
            after: self.after,
            later: [None; CATEGORIES],
            wrap: [None; CATEGORIES],
        }
    }
    /// Advance for inspected blocked/unknown entries as well as eligible ones.
    /// Persistence is a separately budgeted caller operation, never performed
    /// by this facts-only module.
    pub(crate) fn inspected(&mut self, category: WorkCategory, address: CursorAddress) {
        self.after[category as usize] = Some(address);
        self.next_category = ((category as usize + 1) % CATEGORIES) as u8;
    }
}

pub(crate) struct FairSelection {
    next_category: u8,
    after: [Option<CursorAddress>; CATEGORIES],
    later: [Option<CursorAddress>; CATEGORIES],
    wrap: [Option<CursorAddress>; CATEGORIES],
}
impl FairSelection {
    pub(crate) fn observe(&mut self, category: WorkCategory, address: CursorAddress) {
        let index = category as usize;
        let selected = if self.after[index].is_none_or(|after| address > after) {
            &mut self.later[index]
        } else {
            &mut self.wrap[index]
        };
        if selected.is_none_or(|existing| address < existing) {
            *selected = Some(address);
        }
    }
    pub(crate) fn selected(&self) -> Option<(WorkCategory, CursorAddress)> {
        for delta in 0..CATEGORIES {
            let index = (self.next_category as usize + delta) % CATEGORIES;
            if let Some(address) = self.later[index].or(self.wrap[index]) {
                return Some((WorkCategory::from_index(index), address));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn address(number: u64) -> CursorAddress {
        let mut bytes = [0; ADDRESS_BYTES];
        bytes[..8].copy_from_slice(&number.to_be_bytes());
        CursorAddress(bytes)
    }
    #[test]
    fn dc_cursor_restart_blocked_first_and_category_rotation() {
        let mut cursor = MaintenanceCursor::new([42; 32]);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..510 {
            cursor = MaintenanceCursor::decode(&cursor.encode(), [42; 32]).unwrap();
            let mut selection = cursor.selection();
            // Reverse order falsifies dependence on directory enumeration order.
            for n in (0..250).rev() {
                selection.observe(WorkCategory::Canonical, address(n));
            }
            selection.observe(WorkCategory::Run, address(999));
            let (category, candidate) = selection.selected().unwrap();
            cursor.inspected(category, candidate); // candidate 0 remains blocked.
            if category == WorkCategory::Canonical {
                seen.insert(candidate);
            }
        }
        // Alternating categories prevents the large canonical namespace from
        // starving even one blocked run, while canonical resumes after blocked0.
        assert_eq!(seen.len(), 250);
        assert!(seen.contains(&address(249)));
    }
    #[test]
    fn dc_cursor_wrap_and_all_categories() {
        let mut cursor = MaintenanceCursor::new([1; 32]);
        for _ in 0..2 {
            for expected in 0..CATEGORIES {
                let mut selection = cursor.selection();
                for index in (0..CATEGORIES).rev() {
                    selection.observe(WorkCategory::from_index(index), address(1));
                }
                let (category, selected) = selection.selected().unwrap();
                assert_eq!(category as usize, expected);
                cursor.inspected(category, selected);
            }
        }
        assert!(MaintenanceCursor::new([1; 32])
            .selection()
            .selected()
            .is_none());
    }
    #[test]
    fn dc_cursor_codec_corruption_and_recreated_root() {
        let cursor = MaintenanceCursor::new([3; 32]);
        let mut bytes = cursor.encode();
        assert!(matches!(
            MaintenanceCursor::decode(&bytes, [4; 32]),
            Err(CursorFailure::DifferentInstallation)
        ));
        bytes[100] ^= 1;
        assert!(matches!(
            MaintenanceCursor::decode(&bytes, [3; 32]),
            Err(CursorFailure::Corrupt)
        ));
    }
}
