//! Finite native format facts only. These do not certify a producer profile,
//! database origin, loader usability, source stability or retirement authority.
use super::maintenance_budget::CleanupMemory;
use super::maintenance_validation::{ByteSource, ValidationIssue};

const PAGE_BYTES: u64 = 4096;
const HEADER_BYTES: usize = 49;
const INVALID_PAGE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativePageRange {
    pub(crate) start: u32,
    pub(crate) count: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeHeader43Facts {
    pub(crate) catalog: NativePageRange,
    pub(crate) metadata: NativePageRange,
    pub(crate) database_id: [u8; 16],
    pub(crate) data_file_pages: u32,
    pub(crate) available_pages: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeUnknown {
    Source(ValidationIssue),
    UnsupportedLayout,
    Truncated,
    InvalidRange,
    Overflow,
}
impl From<ValidationIssue> for NativeUnknown {
    fn from(value: ValidationIssue) -> Self {
        Self::Source(value)
    }
}

/// Parse only the selected storage43, no-debug, little-endian, 4096-byte-page,
/// format2 header layout. Its structure is not evidence that a program created
/// these bytes. Native page_idx_t is u32; the actual format2 prefix is49 bytes.
pub(crate) fn scan_native_header43<S: ByteSource + ?Sized>(
    source: &S,
    account: &CleanupMemory,
) -> Result<NativeHeader43Facts, NativeUnknown> {
    let floor = HEADER_BYTES
        .checked_add(std::mem::size_of::<NativeHeader43Facts>())
        .and_then(|n| n.checked_add(128))
        .ok_or(NativeUnknown::Overflow)?;
    let _reservation = account.reserve(floor).map_err(ValidationIssue::from)?;
    let length = source.len();
    if length < HEADER_BYTES as u64 {
        return Err(NativeUnknown::Truncated);
    }
    if length % PAGE_BYTES != 0 {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let available_pages = length / PAGE_BYTES;
    let mut bytes = [0; HEADER_BYTES];
    let mut at = 0;
    while at < bytes.len() {
        let count = source.read_at(at as u64, &mut bytes[at..])?;
        if count == 0 || count > bytes.len() - at {
            return Err(NativeUnknown::Truncated);
        }
        at = at.checked_add(count).ok_or(NativeUnknown::Overflow)?;
    }
    if &bytes[..4] != b"LBUG"
        || u64::from_le_bytes(bytes[4..12].try_into().unwrap()) != 43
        || bytes[44] != 2
    {
        return Err(NativeUnknown::UnsupportedLayout);
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let catalog = NativePageRange {
        start: word(12),
        count: word(16),
    };
    let metadata = NativePageRange {
        start: word(20),
        count: word(24),
    };
    check_range(catalog, available_pages)?;
    check_range(metadata, available_pages)?;
    let data_file_pages = word(45);
    if u64::from(data_file_pages) > available_pages {
        return Err(NativeUnknown::InvalidRange);
    }
    Ok(NativeHeader43Facts {
        catalog,
        metadata,
        database_id: bytes[28..44].try_into().unwrap(),
        data_file_pages,
        available_pages,
    })
}
fn check_range(range: NativePageRange, pages: u64) -> Result<(), NativeUnknown> {
    if range.start == INVALID_PAGE {
        return if range.count == 0 {
            Ok(())
        } else {
            Err(NativeUnknown::InvalidRange)
        };
    }
    let start = u64::from(range.start);
    let count = u64::from(range.count);
    if count == 0 || start >= pages || count > pages - start {
        return Err(NativeUnknown::InvalidRange);
    }
    start
        .checked_mul(PAGE_BYTES)
        .and_then(|s| count.checked_mul(PAGE_BYTES).and_then(|n| s.checked_add(n)))
        .ok_or(NativeUnknown::Overflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Vec<u8> {
        let mut bytes = vec![0; 3 * PAGE_BYTES as usize];
        bytes[..4].copy_from_slice(b"LBUG");
        bytes[4..12].copy_from_slice(&43u64.to_le_bytes());
        for (at, value) in [(12, 1u32), (16, 1), (20, 2), (24, 1), (45, 3)] {
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[28..44].copy_from_slice(&[0x55; 16]);
        bytes[44] = 2;
        bytes
    }
    #[test]
    fn dc_native_header43_exact_layout_ranges_and_account() {
        let memory = CleanupMemory::new(4096).unwrap();
        let floor = memory.used();
        let bytes = header();
        let facts = scan_native_header43(bytes.as_slice(), &memory).unwrap();
        assert_eq!(facts.catalog, NativePageRange { start: 1, count: 1 });
        assert_eq!(facts.metadata, NativePageRange { start: 2, count: 1 });
        assert_eq!(facts.database_id, [0x55; 16]);
        assert_eq!(facts.data_file_pages, 3);
        assert_eq!(memory.used(), floor);
        let small = CleanupMemory::new(64).unwrap();
        assert!(matches!(
            scan_native_header43(bytes.as_slice(), &small),
            Err(NativeUnknown::Source(ValidationIssue::Resource(_)))
        ));
    }
    #[test]
    fn dc_native_header43_truncation_sentinel_tail_and_range_rejections() {
        let memory = CleanupMemory::new(4096).unwrap();
        let bytes = header();
        for n in 0..HEADER_BYTES {
            assert_eq!(
                scan_native_header43(&bytes[..n], &memory),
                Err(NativeUnknown::Truncated)
            );
        }
        for (at, value) in [
            (12, u32::MAX),
            (16, 0),
            (16, u32::MAX),
            (20, 3),
            (24, 2),
            (45, 4),
        ] {
            let mut invalid = bytes.clone();
            invalid[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(
                scan_native_header43(invalid.as_slice(), &memory),
                Err(NativeUnknown::InvalidRange)
            );
        }
        let mut empty = bytes.clone();
        for at in [12, 20] {
            empty[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            empty[at + 4..at + 8].fill(0);
        }
        assert!(scan_native_header43(empty.as_slice(), &memory).is_ok());
        for at in [0, 4, 44] {
            let mut invalid = bytes.clone();
            invalid[at] ^= 1;
            assert_eq!(
                scan_native_header43(invalid.as_slice(), &memory),
                Err(NativeUnknown::UnsupportedLayout)
            );
        }
        assert_eq!(
            scan_native_header43(&bytes[..bytes.len() - 1], &memory),
            Err(NativeUnknown::UnsupportedLayout)
        );
    }
    #[test]
    fn dc_native_header43_short_reads_and_inconsistent_sources() {
        struct Short<'a> {
            bytes: &'a [u8],
            stop: usize,
        }
        impl ByteSource for Short<'_> {
            fn len(&self) -> u64 {
                self.bytes.len() as u64
            }
            fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize, ValidationIssue> {
                let at = offset as usize;
                if at >= self.stop {
                    return Ok(0);
                }
                let n = out.len().min(3).min(self.stop - at);
                out[..n].copy_from_slice(&self.bytes[at..at + n]);
                Ok(n)
            }
        }
        let memory = CleanupMemory::new(4096).unwrap();
        let bytes = header();
        assert!(scan_native_header43(
            &Short {
                bytes: &bytes,
                stop: 49
            },
            &memory
        )
        .is_ok());
        for stop in 0..49 {
            assert_eq!(
                scan_native_header43(
                    &Short {
                        bytes: &bytes,
                        stop
                    },
                    &memory
                ),
                Err(NativeUnknown::Truncated)
            );
        }
    }
}
