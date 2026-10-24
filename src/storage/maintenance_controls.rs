//! Fixed prebound control records. Codecs produce facts, never ownership.
//! Commit and Intent occupy independent checksum/write regions; Intent retains
//! the complete next slot bytes so recovery never guesses from highest version.
use sha2::{Digest, Sha256};

pub(crate) const PAYLOAD_BYTES: usize = 2048;
pub(crate) const SLOT_BYTES: usize = 56 + PAYLOAD_BYTES + 32;
pub(crate) const COMMIT_BYTES: usize = 128;
pub(crate) const INTENT_BYTES: usize = 96 + SLOT_BYTES + 32;
pub(crate) const SELECTOR_BYTES: usize = COMMIT_BYTES + INTENT_BYTES;
const SLOT_MAGIC: &[u8; 8] = b"CGSLOT01";
const COMMIT_MAGIC: &[u8; 8] = b"CGCOMM01";
const INTENT_MAGIC: &[u8; 8] = b"CGINTN01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlFailure {
    Size,
    Version,
    Checksum,
    Incarnation,
    Sequence,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotFact {
    pub(crate) generation: u64,
    pub(crate) payload_len: usize,
    pub(crate) digest: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitFact {
    pub(crate) generation: u64,
    pub(crate) active: usize,
    pub(crate) slot_digest: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IntentFact {
    pub(crate) before: u64,
    pub(crate) after: u64,
    pub(crate) target: usize,
    pub(crate) slot_digest: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    Committed(CommitFact),
    RewriteIntendedSlot(IntentFact),
    CommitIntendedSlot(IntentFact),
    Unknown,
}
fn number(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("fixed codec field"))
}
fn finish(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = Sha256::digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn validate<'a>(
    bytes: &'a [u8],
    len: usize,
    magic: &[u8; 8],
    incarnation: &[u8; 32],
) -> Result<&'a [u8], ControlFailure> {
    if bytes.len() != len {
        return Err(ControlFailure::Size);
    }
    if &bytes[..8] != magic {
        return Err(ControlFailure::Version);
    }
    if &bytes[8..40] != incarnation {
        return Err(ControlFailure::Incarnation);
    }
    let end = len - 32;
    if Sha256::digest(&bytes[..end]).as_slice() != &bytes[end..] {
        return Err(ControlFailure::Checksum);
    }
    Ok(bytes)
}
/// Preparation only: caller owns/reserves both source and destination bytes.
pub(crate) fn encode_slot(
    bytes: &mut [u8],
    incarnation: &[u8; 32],
    generation: u64,
    payload: &[u8],
) -> Result<SlotFact, ControlFailure> {
    if bytes.len() != SLOT_BYTES || payload.len() > PAYLOAD_BYTES {
        return Err(ControlFailure::Size);
    }
    bytes.fill(0);
    bytes[..8].copy_from_slice(SLOT_MAGIC);
    bytes[8..40].copy_from_slice(incarnation);
    bytes[40..48].copy_from_slice(&generation.to_le_bytes());
    bytes[48..56].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes[56..56 + payload.len()].copy_from_slice(payload);
    finish(bytes);
    decode_slot(bytes, incarnation)
}
pub(crate) fn decode_slot(
    bytes: &[u8],
    incarnation: &[u8; 32],
) -> Result<SlotFact, ControlFailure> {
    let bytes = validate(bytes, SLOT_BYTES, SLOT_MAGIC, incarnation)?;
    let payload_len = usize::try_from(number(&bytes[48..56])).map_err(|_| ControlFailure::Size)?;
    if payload_len > PAYLOAD_BYTES
        || bytes[56 + payload_len..SLOT_BYTES - 32]
            .iter()
            .any(|b| *b != 0)
    {
        return Err(ControlFailure::Size);
    }
    Ok(SlotFact {
        generation: number(&bytes[40..48]),
        payload_len,
        digest: bytes[SLOT_BYTES - 32..].try_into().unwrap(),
    })
}
pub(crate) fn encode_commit(
    bytes: &mut [u8],
    incarnation: &[u8; 32],
    fact: CommitFact,
) -> Result<(), ControlFailure> {
    if bytes.len() != COMMIT_BYTES || fact.active > 1 {
        return Err(ControlFailure::Size);
    }
    bytes.fill(0);
    bytes[..8].copy_from_slice(COMMIT_MAGIC);
    bytes[8..40].copy_from_slice(incarnation);
    bytes[40..48].copy_from_slice(&fact.generation.to_le_bytes());
    bytes[48..56].copy_from_slice(&(fact.active as u64).to_le_bytes());
    bytes[56..88].copy_from_slice(&fact.slot_digest);
    finish(bytes);
    Ok(())
}
pub(crate) fn decode_commit(
    bytes: &[u8],
    incarnation: &[u8; 32],
) -> Result<CommitFact, ControlFailure> {
    let bytes = validate(bytes, COMMIT_BYTES, COMMIT_MAGIC, incarnation)?;
    let active = usize::try_from(number(&bytes[48..56])).map_err(|_| ControlFailure::Size)?;
    if active > 1 || bytes[88..96].iter().any(|b| *b != 0) {
        return Err(ControlFailure::Size);
    }
    Ok(CommitFact {
        generation: number(&bytes[40..48]),
        active,
        slot_digest: bytes[56..88].try_into().unwrap(),
    })
}
pub(crate) fn encode_intent(
    bytes: &mut [u8],
    incarnation: &[u8; 32],
    current: CommitFact,
    next_slot: &[u8],
) -> Result<IntentFact, ControlFailure> {
    if bytes.len() != INTENT_BYTES || current.active > 1 {
        return Err(ControlFailure::Size);
    }
    let next = decode_slot(next_slot, incarnation)?;
    if current.generation.checked_add(1) != Some(next.generation) {
        return Err(ControlFailure::Sequence);
    }
    bytes.fill(0);
    bytes[..8].copy_from_slice(INTENT_MAGIC);
    bytes[8..40].copy_from_slice(incarnation);
    bytes[40..48].copy_from_slice(&current.generation.to_le_bytes());
    bytes[48..56].copy_from_slice(&next.generation.to_le_bytes());
    bytes[56..64].copy_from_slice(&((1 - current.active) as u64).to_le_bytes());
    bytes[64..96].copy_from_slice(&next.digest);
    bytes[96..96 + SLOT_BYTES].copy_from_slice(next_slot);
    finish(bytes);
    decode_intent(bytes, incarnation)
}
pub(crate) fn decode_intent(
    bytes: &[u8],
    incarnation: &[u8; 32],
) -> Result<IntentFact, ControlFailure> {
    let bytes = validate(bytes, INTENT_BYTES, INTENT_MAGIC, incarnation)?;
    let before = number(&bytes[40..48]);
    let after = number(&bytes[48..56]);
    let target = usize::try_from(number(&bytes[56..64])).map_err(|_| ControlFailure::Size)?;
    let slot = decode_slot(&bytes[96..96 + SLOT_BYTES], incarnation)?;
    if before.checked_add(1) != Some(after)
        || slot.generation != after
        || target > 1
        || slot.digest.as_slice() != &bytes[64..96]
    {
        return Err(ControlFailure::Sequence);
    }
    Ok(IntentFact {
        before,
        after,
        target,
        slot_digest: slot.digest,
    })
}
/// All checksum/codec work is preparation outside actual authority. The later
/// admitted step compares exact prepared bytes/stamps and makes one own effect.
pub(crate) fn recovery_step(
    commit: Option<CommitFact>,
    intent: Option<IntentFact>,
    slots: [Option<SlotFact>; 2],
) -> RecoveryStep {
    if commit.is_some_and(|c| c.active > 1)
        || intent.is_some_and(|i| i.target > 1 || i.before.checked_add(1) != Some(i.after))
    {
        return RecoveryStep::Unknown;
    }
    let matches = |index: usize, generation: u64, digest: [u8; 32]| {
        slots.get(index).is_some_and(|slot| {
            slot.is_some_and(|s| s.generation == generation && s.digest == digest)
        })
    };
    match (commit, intent) {
        (Some(c), None) if matches(c.active, c.generation, c.slot_digest) => {
            RecoveryStep::Committed(c)
        }
        (Some(c), Some(i))
            if c.generation == i.after
                && c.active == i.target
                && c.slot_digest == i.slot_digest
                && matches(c.active, c.generation, c.slot_digest) =>
        {
            RecoveryStep::Committed(c)
        }
        (c, Some(i))
            if c.is_none()
                || c.is_some_and(|c| {
                    c.generation == i.before
                        && c.active == 1 - i.target
                        && matches(c.active, c.generation, c.slot_digest)
                }) =>
        {
            if matches(i.target, i.after, i.slot_digest) {
                RecoveryStep::CommitIntendedSlot(i)
            } else {
                RecoveryStep::RewriteIntendedSlot(i)
            }
        }
        _ => RecoveryStep::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::maintenance_budget::CleanupMemory;

    #[test]
    fn dc_control_codec_reserves_before_bytes_and_preserves_full_intended_slot() {
        let capacity = 2 * SLOT_BYTES + INTENT_BYTES + COMMIT_BYTES;
        let memory = CleanupMemory::new(capacity + 1024).unwrap();
        let baseline = memory.used();
        // The account's own exact allocation and caller's bookkeeping floor
        // share this ceiling; neither is available again to buffer reserves.
        let floor = memory
            .reserve(memory.limit() - baseline - capacity)
            .unwrap();
        let mut old = memory
            .reserve(SLOT_BYTES)
            .unwrap()
            .bytes(SLOT_BYTES)
            .unwrap();
        let mut next = memory
            .reserve(SLOT_BYTES)
            .unwrap()
            .bytes(SLOT_BYTES)
            .unwrap();
        let mut intent = memory
            .reserve(INTENT_BYTES)
            .unwrap()
            .bytes(INTENT_BYTES)
            .unwrap();
        let mut commit = memory
            .reserve(COMMIT_BYTES)
            .unwrap()
            .bytes(COMMIT_BYTES)
            .unwrap();
        let nonce = [17; 32];
        let old_fact = encode_slot(old.as_mut_slice(), &nonce, 12, b"old").unwrap();
        let next_fact =
            encode_slot(next.as_mut_slice(), &nonce, 13, b"complete next payload").unwrap();
        let current = CommitFact {
            generation: 12,
            active: 0,
            slot_digest: old_fact.digest,
        };
        encode_commit(commit.as_mut_slice(), &nonce, current).unwrap();
        assert_eq!(decode_commit(commit.as_slice(), &nonce).unwrap(), current);
        let fact = encode_intent(intent.as_mut_slice(), &nonce, current, next.as_slice()).unwrap();
        assert_eq!(&intent.as_slice()[96..96 + SLOT_BYTES], next.as_slice());
        assert_eq!(fact.after, next_fact.generation);
        assert_eq!(memory.used(), memory.limit());
        assert!(memory.reserve(1).is_err());
        drop((old, next, intent, commit, floor));
        assert_eq!(memory.used(), baseline);
    }

    #[test]
    fn dc_control_independent_regions_recover_every_partial_write_cut() {
        let nonce = [21; 32];
        let mut old = [0; SLOT_BYTES];
        let mut next = [0; SLOT_BYTES];
        let old_slot = encode_slot(&mut old, &nonce, 3, b"old committed value").unwrap();
        let next_slot =
            encode_slot(&mut next, &nonce, 4, b"new exact reconstructible value").unwrap();
        let current = CommitFact {
            generation: 3,
            active: 0,
            slot_digest: old_slot.digest,
        };
        let mut old_commit = [0; COMMIT_BYTES];
        encode_commit(&mut old_commit, &nonce, current).unwrap();
        let mut intent = [0; INTENT_BYTES];
        let i = encode_intent(&mut intent, &nonce, current, &next).unwrap();
        let mut selector = [0; SELECTOR_BYTES];
        selector[..COMMIT_BYTES].copy_from_slice(&old_commit);
        for cut in 0..=INTENT_BYTES {
            selector[COMMIT_BYTES..].fill(0);
            selector[COMMIT_BYTES..COMMIT_BYTES + cut].copy_from_slice(&intent[..cut]);
            // Writing the independent Intent can NEVER destroy old Commit.
            assert_eq!(
                decode_commit(&selector[..COMMIT_BYTES], &nonce).unwrap(),
                current
            );
            let observed = decode_intent(&selector[COMMIT_BYTES..], &nonce).ok();
            let step = recovery_step(Some(current), observed, [Some(old_slot), None]);
            assert_eq!(
                step,
                if observed.is_some() {
                    RecoveryStep::RewriteIntendedSlot(i)
                } else {
                    RecoveryStep::Committed(current)
                }
            );
        }
        for cut in 0..=SLOT_BYTES {
            let mut partial = [0; SLOT_BYTES];
            partial[..cut].copy_from_slice(&next[..cut]);
            let slot = decode_slot(&partial, &nonce).ok();
            assert_eq!(
                recovery_step(Some(current), Some(i), [Some(old_slot), slot]),
                if slot == Some(next_slot) {
                    RecoveryStep::CommitIntendedSlot(i)
                } else {
                    RecoveryStep::RewriteIntendedSlot(i)
                }
            );
        }
        let final_commit = CommitFact {
            generation: 4,
            active: 1,
            slot_digest: next_slot.digest,
        };
        let mut commit = [0; COMMIT_BYTES];
        encode_commit(&mut commit, &nonce, final_commit).unwrap();
        selector[COMMIT_BYTES..].copy_from_slice(&intent);
        for cut in 0..=COMMIT_BYTES {
            selector[..COMMIT_BYTES].copy_from_slice(&old_commit);
            selector[..cut].copy_from_slice(&commit[..cut]);
            assert_eq!(decode_intent(&selector[COMMIT_BYTES..], &nonce).unwrap(), i);
            let observed = decode_commit(&selector[..COMMIT_BYTES], &nonce).ok();
            let step = recovery_step(observed, Some(i), [Some(old_slot), Some(next_slot)]);
            assert_eq!(
                step,
                if observed == Some(final_commit) {
                    RecoveryStep::Committed(final_commit)
                } else {
                    RecoveryStep::CommitIntendedSlot(i)
                }
            );
        }
    }

    #[test]
    fn dc_control_invalid_witnesses_and_unrelated_generations_stay_unknown() {
        let nonce = [9; 32];
        let mut slot = [0; SLOT_BYTES];
        let s = encode_slot(&mut slot, &nonce, u64::MAX, b"last version").unwrap();
        let c = CommitFact {
            generation: u64::MAX,
            active: 0,
            slot_digest: s.digest,
        };
        let mut intent = [0; INTENT_BYTES];
        assert_eq!(
            encode_intent(&mut intent, &nonce, c, &slot),
            Err(ControlFailure::Sequence)
        );
        assert_eq!(
            decode_slot(&slot, &[8; 32]),
            Err(ControlFailure::Incarnation)
        );
        for index in [0, 7, 8, 39, 40, 55, 56, SLOT_BYTES - 1] {
            let mut corrupt = slot;
            corrupt[index] ^= 1;
            assert!(decode_slot(&corrupt, &nonce).is_err());
        }
        assert_eq!(
            recovery_step(None, None, [Some(s), Some(s)]),
            RecoveryStep::Unknown
        );
        let foreign = CommitFact {
            generation: 99,
            active: 0,
            slot_digest: s.digest,
        };
        let i = IntentFact {
            before: 7,
            after: 8,
            target: 1,
            slot_digest: s.digest,
        };
        assert_eq!(
            recovery_step(Some(foreign), Some(i), [Some(s), Some(s)]),
            RecoveryStep::Unknown
        );
        assert_eq!(
            decode_slot(&slot[..SLOT_BYTES - 1], &nonce),
            Err(ControlFailure::Size)
        );
        let mut too_small = [0; SLOT_BYTES - 1];
        assert_eq!(
            encode_slot(&mut too_small, &nonce, 0, b""),
            Err(ControlFailure::Size)
        );
    }
}

pub(crate) const BINDING_BYTES: usize = 512;
pub(crate) const DATA_NAMES: [&[u8]; 2] = [b".publication.data0", b".publication.data1"];
pub(crate) const SELECTOR_NAME: &[u8] = b".publication.selector";
pub(crate) const STREAM_NAME: &[u8] = b".publication.stream";
pub(crate) const BINDING_NAME: &[u8] = b".publication.binding";
const BINDING_MAGIC: &[u8; 8] = b"CGBIND01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PublisherBinding {
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) coordination: super::locks::FileIdentity,
    pub(crate) data: [super::locks::FileIdentity; 2],
    pub(crate) selector: super::locks::FileIdentity,
    pub(crate) stream: Option<super::locks::FileIdentity>,
    pub(crate) incarnation: [u8; 32],
}
fn identity_into(bytes: &mut [u8], identity: super::locks::FileIdentity) {
    let (volume, index) = identity.fixed_words();
    bytes[..8].copy_from_slice(&volume.to_le_bytes());
    bytes[8..16].copy_from_slice(&index.to_le_bytes());
}
fn identity_from(bytes: &[u8]) -> super::locks::FileIdentity {
    super::locks::FileIdentity::from_fixed_words(number(&bytes[..8]), number(&bytes[8..16]))
}
impl PublisherBinding {
    pub(crate) fn encode(&self, bytes: &mut [u8]) -> Result<(), ControlFailure> {
        if bytes.len() != BINDING_BYTES {
            return Err(ControlFailure::Size);
        }
        bytes.fill(0);
        bytes[..8].copy_from_slice(BINDING_MAGIC);
        bytes[8..16].copy_from_slice(&1u64.to_le_bytes());
        bytes[16..48].copy_from_slice(&self.incarnation);
        for (offset, identity) in [
            (48, self.root),
            (64, self.coordination),
            (80, self.data[0]),
            (96, self.data[1]),
            (112, self.selector),
        ] {
            identity_into(&mut bytes[offset..offset + 16], identity);
        }
        if let Some(stream) = self.stream {
            bytes[128..136].copy_from_slice(&1u64.to_le_bytes());
            identity_into(&mut bytes[136..152], stream);
        }
        finish(bytes);
        Ok(())
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ControlFailure> {
        if bytes.len() != BINDING_BYTES {
            return Err(ControlFailure::Size);
        }
        if &bytes[..8] != BINDING_MAGIC || number(&bytes[8..16]) != 1 {
            return Err(ControlFailure::Version);
        }
        if Sha256::digest(&bytes[..BINDING_BYTES - 32]).as_slice() != &bytes[BINDING_BYTES - 32..] {
            return Err(ControlFailure::Checksum);
        }
        if bytes[152..BINDING_BYTES - 32].iter().any(|b| *b != 0) {
            return Err(ControlFailure::Size);
        }
        let stream = match number(&bytes[128..136]) {
            0 if bytes[136..152].iter().all(|b| *b == 0) => None,
            1 => Some(identity_from(&bytes[136..152])),
            _ => return Err(ControlFailure::Version),
        };
        Ok(Self {
            root: identity_from(&bytes[48..64]),
            coordination: identity_from(&bytes[64..80]),
            data: [
                identity_from(&bytes[80..96]),
                identity_from(&bytes[96..112]),
            ],
            selector: identity_from(&bytes[112..128]),
            stream,
            incarnation: bytes[16..48].try_into().unwrap(),
        })
    }
}

/// Actual simultaneous PRIMARY factory storage. No heap/account is hidden in
/// this type. Cleanup preparation separately reserves every concurrent image.
pub(crate) struct PublicationFactoryScratch {
    slot: [u8; SLOT_BYTES],
    selector: [u8; SELECTOR_BYTES],
    binding: [u8; BINDING_BYTES],
    readback: [u8; SELECTOR_BYTES],
}
impl PublicationFactoryScratch {
    pub(crate) fn new() -> Self {
        Self {
            slot: [0; SLOT_BYTES],
            selector: [0; SELECTOR_BYTES],
            binding: [0; BINDING_BYTES],
            readback: [0; SELECTOR_BYTES],
        }
    }
}
fn factory_write(
    root: &super::locks::RootedDirectory,
    name: &[u8],
    bytes: &[u8],
    readback: &mut [u8],
) -> std::io::Result<super::locks::FileIdentity> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut file = root.create_new(name)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if file.metadata()?.len() != bytes.len() as u64 {
        return Err(invalid_control_io());
    }
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut readback[..bytes.len()])?;
    if &readback[..bytes.len()] != bytes {
        return Err(invalid_control_io());
    }
    let identity = super::locks::file_identity(&file).map_err(control_native_io)?;
    let named = root.open_existing(name)?;
    if super::locks::file_identity(&named).map_err(control_native_io)? != identity {
        return Err(invalid_control_io());
    }
    Ok(identity)
}
fn control_native_io(error: crate::error::NativeError) -> std::io::Error {
    match error {
        crate::error::NativeError::Io(error) => error,
        _ => invalid_control_io(),
    }
}
/// The only caller holds genuine fresh-root evidence minted in the actual
/// create_dir success branch. Existing producer/acquisition has no constructor.
/// An error leaves finite unknown names; callers keep primary work optional.
pub(crate) fn initialize_publication_controls(
    fresh: &super::maintenance_ownership::FreshPublicationRoot<'_>,
    scratch: &mut PublicationFactoryScratch,
) -> std::io::Result<PublisherBinding> {
    initialize_creation_controls(fresh, scratch, b"CGBUILD1")
}
pub(super) fn initialize_creation_controls(
    fresh: &super::maintenance_ownership::FreshPublicationRoot<'_>,
    scratch: &mut PublicationFactoryScratch,
    initial: &[u8],
) -> std::io::Result<PublisherBinding> {
    let root = fresh.anchor();
    let nonce = fresh.incarnation();
    let first =
        encode_slot(&mut scratch.slot, &nonce, 0, initial).map_err(|_| invalid_control_io())?;
    let data0 = factory_write(root, DATA_NAMES[0], &scratch.slot, &mut scratch.readback)?;
    encode_slot(&mut scratch.slot, &nonce, 0, b"").map_err(|_| invalid_control_io())?;
    let data1 = factory_write(root, DATA_NAMES[1], &scratch.slot, &mut scratch.readback)?;
    scratch.selector.fill(0);
    encode_commit(
        &mut scratch.selector[..COMMIT_BYTES],
        &nonce,
        CommitFact {
            generation: 0,
            active: 0,
            slot_digest: first.digest,
        },
    )
    .map_err(|_| invalid_control_io())?;
    let selector = factory_write(
        root,
        SELECTOR_NAME,
        &scratch.selector,
        &mut scratch.readback,
    )?;
    // Empty initial stream proves one inode only, no journal/source/version.
    let stream = factory_write(root, STREAM_NAME, b"", &mut scratch.readback)?;
    root.sync_namespace()?;
    let binding = PublisherBinding {
        root: root.identity(),
        coordination: fresh.coordination_identity()?,
        data: [data0, data1],
        selector,
        stream: Some(stream),
        incarnation: nonce,
    };
    binding
        .encode(&mut scratch.binding)
        .map_err(|_| invalid_control_io())?;
    factory_write(root, BINDING_NAME, &scratch.binding, &mut scratch.readback)?;
    root.sync_namespace()?;
    Ok(binding)
}

/// Ordinary PRODUCER publication namespace ownership. This is never a janitor
/// mutation seal; its preparation/revalidation may parse fixed control codecs.
pub(crate) struct RootPublicationLease<'a, 'root> {
    selector: super::locks::FixedLockedFile,
    directory: super::locks::RootedDirectory,
    capability: &'a mut (dyn super::maintenance_ownership::PublicationRootAuthority + 'root),
    binding: PublisherBinding,
    binding_identity: super::locks::FileIdentity,
    commit: CommitFact,
    usable: bool,
}
impl RootPublicationLease<'_, '_> {
    #[cfg(test)]
    pub(crate) fn directory(&self) -> &super::locks::RootedDirectory {
        &self.directory
    }
    #[cfg(test)]
    pub(crate) fn root(&self) -> &std::path::Path {
        self.capability.root()
    }
    pub(crate) fn binding(&self) -> &PublisherBinding {
        &self.binding
    }
    #[cfg(test)]
    pub(crate) fn committed(&self) -> CommitFact {
        self.commit
    }
    pub(crate) fn publication_binding_identity(&self) -> super::locks::FileIdentity {
        self.binding_identity
    }
    pub(crate) fn root_cohort_identity(&self) -> super::locks::FileIdentity {
        self.capability.cohort_identity()
    }
    /// Producer-only read of the exact committed fixed payload. Domain codecs
    /// must reject unknown/nonterminal state before choosing a new operation.
    pub(crate) fn read_primary_control(
        &self,
        output: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<usize, crate::error::NativeError> {
        if !self.revalidate_existing()? {
            return Err(invalid_control_io().into());
        }
        let file = self
            .directory
            .open_readonly_file(DATA_NAMES[self.commit.active])?;
        if super::locks::file_identity(&file)? != self.binding.data[self.commit.active] {
            return Err(invalid_control_io().into());
        }
        let image = read_exact_image::<SLOT_BYTES>(&file)?;
        let fact =
            decode_slot(&image, &self.binding.incarnation).map_err(|_| invalid_control_io())?;
        if fact.generation != self.commit.generation || fact.digest != self.commit.slot_digest {
            return Err(invalid_control_io().into());
        }
        output.fill(0);
        output[..fact.payload_len].copy_from_slice(&image[56..56 + fact.payload_len]);
        Ok(fact.payload_len)
    }
    pub(crate) fn revalidate_existing(&self) -> Result<bool, crate::error::NativeError> {
        if !self.usable || !self.capability.revalidate()? {
            return Ok(false);
        }
        let named = self.directory.open_readonly_file(SELECTOR_NAME)?;
        let binding = self.directory.open_readonly_file(BINDING_NAME)?;
        Ok(
            super::locks::file_identity(&binding)? == self.binding_identity
                && read_exact_image::<BINDING_BYTES>(&binding)
                    .ok()
                    .and_then(|bytes| PublisherBinding::decode(&bytes).ok())
                    == Some(self.binding)
                && super::locks::file_identity(&named)? == self.binding.selector
                && super::locks::file_identity(self.selector.file())? == self.binding.selector
                && read_exact_image::<SELECTOR_BYTES>(self.selector.file())
                    .ok()
                    .and_then(|bytes| {
                        decode_commit(&bytes[..COMMIT_BYTES], &self.binding.incarnation).ok()
                    })
                    == Some(self.commit)
                && self.selector.mode() == super::locks::LockMode::Exclusive
                && self.directory.identity() == self.binding.root,
        )
    }
}
fn read_exact_image<const N: usize>(file: &std::fs::File) -> std::io::Result<[u8; N]> {
    use std::io::{Read, Seek, SeekFrom};
    if file.metadata()?.len() != N as u64 {
        return Err(invalid_control_io());
    }
    let stamp = super::locks::FixedFileStamp::capture(file, super::locks::FixedControlKind::File)
        .map_err(|_| invalid_control_io())?;
    let mut bytes = [0; N];
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(0))?;
    cursor.read_exact(&mut bytes)?;
    stamp.recheck(file).map_err(|_| invalid_control_io())?;
    Ok(bytes)
}
/// No creation, repair, adoption or new publisher on an existing root. Missing,
/// incomplete or noncommitted namespace returns no optional attestation. The
/// caller must retain its approved safe untracked primary-publication lanes.
pub(crate) fn try_begin_root_publication<'a, 'root>(
    capability: &'a mut super::maintenance_ownership::ArtifactRootProducerCapability<'root>,
) -> Result<Option<RootPublicationLease<'a, 'root>>, crate::error::NativeError> {
    try_begin_publication(capability)
}
pub(super) fn try_begin_publication<'a, 'root>(
    capability: &'a mut (dyn super::maintenance_ownership::PublicationRootAuthority + 'root),
) -> Result<Option<RootPublicationLease<'a, 'root>>, crate::error::NativeError> {
    if !capability.revalidate()? {
        return Ok(None);
    }
    let directory =
        super::locks::RootedDirectory::open(capability.root(), capability.root_identity())?;
    let binding_file = match directory.open_readonly_file(BINDING_NAME) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let binding_bytes = match read_exact_image::<BINDING_BYTES>(&binding_file) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let binding = match PublisherBinding::decode(&binding_bytes) {
        Ok(binding) => binding,
        Err(_) => return Ok(None),
    };
    if binding.root != capability.root_identity()
        || binding.coordination != capability.coordination_identity()
        || binding.incarnation != capability.incarnation()
    {
        return Ok(None);
    }
    let selector_file = directory.open_existing(SELECTOR_NAME)?;
    if super::locks::file_identity(&selector_file)? != binding.selector {
        return Ok(None);
    }
    let Some(selector) =
        super::locks::FixedLockedFile::try_lock(selector_file, super::locks::LockMode::Exclusive)
            .map_err(|error| match error {
            super::locks::FixedControlError::Io(error) => crate::error::NativeError::Io(error),
            _ => crate::error::NativeError::Io(invalid_control_io()),
        })?
    else {
        return Ok(None);
    };
    let named_binding = directory.open_readonly_file(BINDING_NAME)?;
    if super::locks::file_identity(&named_binding)? != super::locks::file_identity(&binding_file)?
        || read_exact_image::<BINDING_BYTES>(&named_binding)
            .ok()
            .as_ref()
            != Some(&binding_bytes)
    {
        return Ok(None);
    }
    let selector_bytes = match read_exact_image::<SELECTOR_BYTES>(selector.file()) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let mut slots = [None; 2];
    for index in 0..2 {
        let file = directory.open_readonly_file(DATA_NAMES[index])?;
        if super::locks::file_identity(&file)? != binding.data[index] {
            return Ok(None);
        }
        let image = match read_exact_image::<SLOT_BYTES>(&file) {
            Ok(image) => image,
            Err(_) => return Ok(None),
        };
        slots[index] = decode_slot(&image, &binding.incarnation).ok();
    }
    if let Some(expected) = binding.stream {
        let stream = directory.open_readonly_file(STREAM_NAME)?;
        if super::locks::file_identity(&stream)? != expected {
            return Ok(None);
        }
    }
    let commit = decode_commit(&selector_bytes[..COMMIT_BYTES], &binding.incarnation).ok();
    let intent = decode_intent(&selector_bytes[COMMIT_BYTES..], &binding.incarnation).ok();
    let RecoveryStep::Committed(commit) = recovery_step(commit, intent, slots) else {
        return Ok(None);
    };
    let binding_identity = super::locks::file_identity(&binding_file)?;
    let lease = RootPublicationLease {
        selector,
        directory,
        capability,
        binding,
        binding_identity,
        commit,
        usable: true,
    };
    if !lease.revalidate_existing()? {
        return Ok(None);
    }
    Ok(Some(lease))
}

/// Primary-operation fixed images; no heap/second cleanup account. A janitor
/// prepares equivalent borrowed slices from its SAME accounted offslot Work.
pub(crate) struct PrimaryPublicationScratch {
    slot: [u8; SLOT_BYTES],
    intent: [u8; INTENT_BYTES],
    commit: [u8; COMMIT_BYTES],
    readback: [u8; SELECTOR_BYTES],
}
impl PrimaryPublicationScratch {
    pub(crate) fn new() -> Self {
        Self {
            slot: [0; SLOT_BYTES],
            intent: [0; INTENT_BYTES],
            commit: [0; COMMIT_BYTES],
            readback: [0; SELECTOR_BYTES],
        }
    }
}
fn primary_range(
    file: &std::fs::File,
    offset: u64,
    bytes: &[u8],
    readback: &mut [u8],
) -> std::io::Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(offset))?;
    cursor.write_all(bytes)?;
    cursor.sync_all()?;
    cursor.seek(SeekFrom::Start(offset))?;
    cursor.read_exact(&mut readback[..bytes.len()])?;
    if &readback[..bytes.len()] != bytes {
        return Err(invalid_control_io());
    }
    Ok(())
}
impl RootPublicationLease<'_, '_> {
    /// Ordinary producer only, outside janitor authority/budget. Publish a fixed
    /// creation/progress intent before the corresponding S3 namespace effect.
    /// The complete next slot is durable in Intent; each region preserves its
    /// independent opposite witness. An interrupted update requires recovery;
    /// this method never silently adopts a newer generation.
    pub(crate) fn publish_primary_control(
        &mut self,
        payload: &[u8],
        scratch: &mut PrimaryPublicationScratch,
    ) -> Result<CommitFact, crate::error::NativeError> {
        if !self.revalidate_existing()? {
            return Err(invalid_control_io().into());
        }
        let selector = read_exact_image::<SELECTOR_BYTES>(self.selector.file())?;
        if decode_commit(&selector[..COMMIT_BYTES], &self.binding.incarnation).ok()
            != Some(self.commit)
        {
            return Err(invalid_control_io().into());
        }
        let generation = self
            .commit
            .generation
            .checked_add(1)
            .ok_or_else(invalid_control_io)?;
        let slot = encode_slot(
            &mut scratch.slot,
            &self.binding.incarnation,
            generation,
            payload,
        )
        .map_err(|_| invalid_control_io())?;
        let intent = encode_intent(
            &mut scratch.intent,
            &self.binding.incarnation,
            self.commit,
            &scratch.slot,
        )
        .map_err(|_| invalid_control_io())?;
        let next = CommitFact {
            generation,
            active: intent.target,
            slot_digest: slot.digest,
        };
        encode_commit(&mut scratch.commit, &self.binding.incarnation, next)
            .map_err(|_| invalid_control_io())?;
        let data = self.directory.open_existing(DATA_NAMES[next.active])?;
        if super::locks::file_identity(&data)? != self.binding.data[next.active]
            || data.metadata()?.len() != SLOT_BYTES as u64
        {
            return Err(invalid_control_io().into());
        }
        // Any following error requires exact recovery under a fresh lease;
        // an old live object cannot replace a partially published Intent.
        self.usable = false;
        primary_range(
            self.selector.file(),
            COMMIT_BYTES as u64,
            &scratch.intent,
            &mut scratch.readback,
        )?;
        primary_range(&data, 0, &scratch.slot, &mut scratch.readback)?;
        primary_range(
            self.selector.file(),
            0,
            &scratch.commit,
            &mut scratch.readback,
        )?;
        self.commit = next;
        self.usable = true;
        Ok(next)
    }
}

#[cfg(test)]
mod factory_codec_tests {
    use super::*;
    use crate::storage::locks::FileIdentity;
    #[test]
    fn dc_control_binding_modes_are_explicit_and_reserve_unknown_bytes() {
        let id = FileIdentity::from_fixed_words(5, 7);
        let mut binding = PublisherBinding {
            root: id,
            coordination: id,
            data: [id, id],
            selector: id,
            stream: None,
            incarnation: [3; 32],
        };
        let mut bytes = [0; BINDING_BYTES];
        binding.encode(&mut bytes).unwrap();
        assert_eq!(PublisherBinding::decode(&bytes).unwrap(), binding);
        binding.stream = Some(FileIdentity::from_fixed_words(0, 0));
        binding.encode(&mut bytes).unwrap();
        assert_eq!(
            PublisherBinding::decode(&bytes).unwrap().stream,
            binding.stream
        );
        bytes[128..136].copy_from_slice(&2u64.to_le_bytes());
        finish(&mut bytes);
        assert!(PublisherBinding::decode(&bytes).is_err());
        binding.stream = None;
        binding.encode(&mut bytes).unwrap();
        bytes[136] = 1;
        finish(&mut bytes);
        assert!(PublisherBinding::decode(&bytes).is_err());
        binding.encode(&mut bytes).unwrap();
        bytes[200] = 1;
        finish(&mut bytes);
        assert!(PublisherBinding::decode(&bytes).is_err());
        assert_eq!(std::mem::size_of::<PublicationFactoryScratch>(), 7432);
    }
}

fn invalid_control_io() -> std::io::Error {
    std::io::ErrorKind::InvalidData.into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WorkspacePhase {
    Building = 0,
    NeedsRecovery = 1,
    OwnerTerminal = 2,
    Closing = 3,
    Quiescent = 4,
    Retiring = 5,
    Removing = 6,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WorkspaceDisposition {
    None = 0,
    PublishedConsumed = 1,
    AbortedNotPublished = 2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceMetadataCreator {
    identity: super::locks::FileIdentity,
    operation: [u8; 32],
    len: u64,
    digest: [u8; 32],
    receipt: Option<super::locks::FileIdentity>,
}
const ROOT_METADATA_NAMES: [&[u8]; 3] = [
    b"journal.json",
    b"cleanup-barrier.json",
    b".reference-scope.json",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceControl {
    pub(crate) phase: WorkspacePhase,
    pub(crate) disposition: WorkspaceDisposition,
    pub(crate) source_epoch: u64,
    pub(crate) admission_epoch: u64,
    pub(crate) child_generation: u64,
    pub(crate) registered: u64,
    pub(crate) closed: u64,
    pub(crate) sealed_source: u64,
    metadata_seal: [u8; 32],
    metadata_creators: [Option<WorkspaceMetadataCreator>; 3],
}
impl WorkspaceControl {
    pub(crate) fn building() -> Self {
        Self {
            phase: WorkspacePhase::Building,
            disposition: WorkspaceDisposition::None,
            source_epoch: 0,
            admission_epoch: 0,
            child_generation: 0,
            registered: 0,
            closed: 0,
            sealed_source: 0,
            metadata_seal: [0; 32],
            metadata_creators: [None; 3],
        }
    }
    pub(crate) fn encode(self) -> [u8; 456] {
        let mut bytes = [0; 456];
        bytes[..8].copy_from_slice(b"CGWCTL01");
        bytes[8] = self.phase as u8;
        bytes[9] = self.disposition as u8;
        for (index, value) in [
            self.source_epoch,
            self.admission_epoch,
            self.child_generation,
            self.registered,
            self.closed,
            self.sealed_source,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[16 + index * 8..24 + index * 8].copy_from_slice(&value.to_le_bytes());
        }
        bytes[64..96].copy_from_slice(&self.metadata_seal);
        for (i, creator) in self.metadata_creators.into_iter().enumerate() {
            if let Some(c) = creator {
                let base = 96 + i * 120;
                bytes[base] = 1;
                identity_into(&mut bytes[base + 8..base + 24], c.identity);
                bytes[base + 24..base + 56].copy_from_slice(&c.operation);
                bytes[base + 56..base + 64].copy_from_slice(&c.len.to_le_bytes());
                bytes[base + 64..base + 96].copy_from_slice(&c.digest);
                if let Some(id) = c.receipt {
                    bytes[base + 96] = 1;
                    identity_into(&mut bytes[base + 104..base + 120], id);
                }
            }
        }
        bytes
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ControlFailure> {
        if bytes.len() != 456 || &bytes[..8] != b"CGWCTL01" || bytes[10..16].iter().any(|b| *b != 0)
        {
            return Err(ControlFailure::Version);
        }
        let phase = match bytes[8] {
            0 => WorkspacePhase::Building,
            1 => WorkspacePhase::NeedsRecovery,
            2 => WorkspacePhase::OwnerTerminal,
            3 => WorkspacePhase::Closing,
            4 => WorkspacePhase::Quiescent,
            5 => WorkspacePhase::Retiring,
            6 => WorkspacePhase::Removing,
            _ => return Err(ControlFailure::Version),
        };
        let disposition = match bytes[9] {
            0 => WorkspaceDisposition::None,
            1 => WorkspaceDisposition::PublishedConsumed,
            2 => WorkspaceDisposition::AbortedNotPublished,
            _ => return Err(ControlFailure::Version),
        };
        let mut numbers = [0; 6];
        for (index, value) in numbers.iter_mut().enumerate() {
            *value = number(&bytes[16 + index * 8..24 + index * 8]);
        }
        if numbers[4] > numbers[3]
            || (matches!(
                phase,
                WorkspacePhase::Building | WorkspacePhase::NeedsRecovery
            ) && disposition != WorkspaceDisposition::None)
            || (matches!(
                phase,
                WorkspacePhase::OwnerTerminal
                    | WorkspacePhase::Closing
                    | WorkspacePhase::Quiescent
                    | WorkspacePhase::Retiring
                    | WorkspacePhase::Removing
            ) && disposition == WorkspaceDisposition::None)
        {
            return Err(ControlFailure::Sequence);
        }
        Ok(Self {
            phase,
            disposition,
            source_epoch: numbers[0],
            admission_epoch: numbers[1],
            child_generation: numbers[2],
            registered: numbers[3],
            closed: numbers[4],
            sealed_source: numbers[5],
            metadata_seal: bytes[64..96].try_into().unwrap(),
            metadata_creators: {
                let mut result = [None; 3];
                for (i, out) in result.iter_mut().enumerate() {
                    let base = 96 + i * 120;
                    let entry = &bytes[base..base + 120];
                    if entry[0] == 0 {
                        if entry.iter().any(|x| *x != 0) {
                            return Err(ControlFailure::Version);
                        }
                    } else {
                        if entry[0] != 1
                            || entry[1..8].iter().chain(&entry[97..104]).any(|x| *x != 0)
                            || entry[96] > 1
                            || (entry[96] == 0 && entry[104..120].iter().any(|x| *x != 0))
                        {
                            return Err(ControlFailure::Version);
                        }
                        *out = Some(WorkspaceMetadataCreator {
                            identity: identity_from(&entry[8..24]),
                            operation: entry[24..56].try_into().unwrap(),
                            len: number(&entry[56..64]),
                            digest: entry[64..96].try_into().unwrap(),
                            receipt: (entry[96] == 1).then(|| identity_from(&entry[104..120])),
                        });
                    }
                }
                result
            },
        })
    }
}

fn workspace_metadata_seal(
    root: &std::path::Path,
    id: super::locks::FileIdentity,
) -> Result<[u8; 32], crate::error::NativeError> {
    let directory = super::locks::RootedDirectory::open_readonly_root(root, id)?;
    let mut hash = Sha256::new();
    hash.update(b"CGW-METADATA-01");
    for name in [
        b".workspace.binding".as_slice(),
        b".workspace.domain.json",
        b".workspace.parent.json",
        b".reference-scope.json",
        b"journal.json",
        b"cleanup-barrier.json",
        b".publication.binding",
    ] {
        hash.update(name);
        match directory.open_readonly_file(name) {
            Ok(file) => {
                let stamp = super::locks::FixedFileStamp::capture(
                    &file,
                    super::locks::FixedControlKind::File,
                )
                .map_err(|_| invalid_control_io())?;
                if stamp.len() > 8 * 1024 * 1024 {
                    return Err(invalid_control_io().into());
                }
                hash.update([1]);
                let (volume, index) = stamp.identity().fixed_words();
                hash.update(volume.to_le_bytes());
                hash.update(index.to_le_bytes());
                hash.update(stamp.len().to_le_bytes());
                let mut bytes = [0u8; 8192];
                let mut offset = 0u64;
                while offset < stamp.len() {
                    let len = (stamp.len() - offset).min(bytes.len() as u64) as usize;
                    super::locks::read_fixed_range(&file, offset, &mut bytes[..len])?;
                    hash.update(&bytes[..len]);
                    offset += len as u64;
                }
                stamp.recheck(&file).map_err(|_| invalid_control_io())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => hash.update([0]),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(hash.finalize().into())
}

const WORKSPACE_SCOPE_NAME: &[u8] = b".workspace.binding";
const WORKSPACE_DOMAIN_NAME: &[u8] = b".workspace.domain.json";
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct WorkspaceDirectoryIncarnation {
    identity: super::locks::FileIdentity,
    nonce: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct WorkspaceBinding {
    root: super::locks::FileIdentity,
    owner: super::locks::FileIdentity,
    registry: super::locks::FileIdentity,
    parent_root: super::locks::FileIdentity,
    parent_coordination: super::locks::FileIdentity,
    parent_cohort: super::locks::FileIdentity,
    publication_binding: super::locks::FileIdentity,
    nonce: [u8; 32],
    parent_nonce: [u8; 32],
    kind: u8,
    writers: super::locks::FileIdentity,
    origins: super::locks::FileIdentity,
    parent_record: Option<super::locks::FileIdentity>,
    startup_gate: super::locks::FileIdentity,
    created_directories: Option<[WorkspaceDirectoryIncarnation; 2]>,
    origins_nonce: [u8; 32],
}
impl WorkspaceBinding {
    fn encode(self) -> [u8; 512] {
        let mut bytes = [0u8; 512];
        bytes[..8].copy_from_slice(b"CGWSCP01");
        bytes[8] = self.kind;
        if let Some(parent) = self.parent_record {
            bytes[9] = 1;
            identity_into(&mut bytes[224..240], parent);
        }
        identity_into(&mut bytes[240..256], self.startup_gate);
        if let Some(directories) = self.created_directories {
            bytes[10] = 1;
            identity_into(&mut bytes[256..272], directories[0].identity);
            bytes[288..320].copy_from_slice(&directories[0].nonce);
            identity_into(&mut bytes[272..288], directories[1].identity);
            bytes[320..352].copy_from_slice(&directories[1].nonce);
        }
        for (index, id) in [
            self.root,
            self.owner,
            self.registry,
            self.parent_root,
            self.parent_coordination,
            self.parent_cohort,
            self.publication_binding,
        ]
        .into_iter()
        .enumerate()
        {
            identity_into(&mut bytes[16 + index * 16..32 + index * 16], id);
        }
        bytes[128..160].copy_from_slice(&self.nonce);
        bytes[160..192].copy_from_slice(&self.parent_nonce);
        identity_into(&mut bytes[192..208], self.writers);
        identity_into(&mut bytes[208..224], self.origins);
        bytes[352..384].copy_from_slice(&self.origins_nonce);
        finish(&mut bytes);
        bytes
    }
    fn decode(bytes: &[u8]) -> Result<Self, ControlFailure> {
        if bytes.len() != 512
            || &bytes[..8] != b"CGWSCP01"
            || !matches!(bytes[8], 1..=3)
            || bytes[9] > 1
            || bytes[10] > 1
            || bytes[11..16]
                .iter()
                .chain(&bytes[384..480])
                .any(|b| *b != 0)
            || (bytes[10] == 0 && bytes[256..352].iter().any(|b| *b != 0))
            || (bytes[8] == 1 && bytes[10] != 1)
            || (bytes[9] == 0 && bytes[224..240].iter().any(|b| *b != 0))
            || (bytes[8] == 3 && bytes[9] != 1)
            || Sha256::digest(&bytes[..480]).as_slice() != &bytes[480..]
        {
            return Err(ControlFailure::Version);
        }
        Ok(Self {
            kind: bytes[8],
            root: identity_from(&bytes[16..32]),
            owner: identity_from(&bytes[32..48]),
            registry: identity_from(&bytes[48..64]),
            parent_root: identity_from(&bytes[64..80]),
            parent_coordination: identity_from(&bytes[80..96]),
            parent_cohort: identity_from(&bytes[96..112]),
            publication_binding: identity_from(&bytes[112..128]),
            nonce: bytes[128..160].try_into().unwrap(),
            parent_nonce: bytes[160..192].try_into().unwrap(),
            writers: identity_from(&bytes[192..208]),
            origins: identity_from(&bytes[208..224]),
            parent_record: (bytes[9] == 1).then(|| identity_from(&bytes[224..240])),
            startup_gate: identity_from(&bytes[240..256]),
            origins_nonce: bytes[352..384].try_into().unwrap(),
            created_directories: (bytes[10] == 1).then(|| {
                [
                    WorkspaceDirectoryIncarnation {
                        identity: identity_from(&bytes[256..272]),
                        nonce: bytes[288..320].try_into().unwrap(),
                    },
                    WorkspaceDirectoryIncarnation {
                        identity: identity_from(&bytes[272..288]),
                        nonce: bytes[320..352].try_into().unwrap(),
                    },
                ]
            }),
        })
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct WorkspaceParentRecord {
    root: std::path::PathBuf,
    scope: WorkspaceBinding,
    scope_file: super::locks::FileIdentity,
}
/// Factory provenance only. Operations still borrow actual live owner/activity
/// and startup gates; this object cannot itself lock, start or delete payloads.
#[derive(Debug)]
pub(crate) struct WorkspaceRegistration {
    root: std::path::PathBuf,
    gate: std::path::PathBuf,
    scope: WorkspaceBinding,
    scope_file: super::locks::FileIdentity,
    domain: super::maintenance_ownership::ReferenceDomain,
}
impl WorkspaceRegistration {
    pub(crate) fn create(
        fresh: &super::layout::FreshReferenceRoot,
        domain: &super::maintenance_ownership::ReferenceDomain,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        owner: &super::locks::LockedFile,
        kind: u8,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        Self::create_with_parent(fresh, domain, mutation, owner, kind, None)
    }
    pub(crate) fn create_with_parent(
        fresh: &super::layout::FreshReferenceRoot,
        domain: &super::maintenance_ownership::ReferenceDomain,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        owner: &super::locks::LockedFile,
        kind: u8,
        parent: Option<&super::run_workspace::ParentRunScope>,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        use std::io::Write;
        if kind == 3 && parent.is_none() {
            return Ok(None);
        }
        if let Some(parent) = parent {
            let registration = parent.registration();
            if !registration.revalidate()?
                || !registration.domain.same_scope(domain)
                || !parent.owner().verifies(
                    &registration.root.join("lease.lock"),
                    super::locks::LockMode::Exclusive,
                )?
            {
                return Ok(None);
            }
            registration.startup_binding().check_building()?;
        }
        let Some(created) = super::maintenance_ownership::initialize_workspace_publisher(
            fresh, domain, mutation, owner, kind,
        )?
        else {
            return Ok(None);
        };
        let writers = super::locks::create_new_coordination_in(
            &created.anchor,
            b".workspace-writers.lock",
            &fresh.path().join(".workspace-writers.lock"),
        )?;
        writers.initialize_creation_incarnation(created.nonce)?;
        let origins = created.anchor.create_directory(b".workspace-origins")?;
        let mut origins_nonce = [0; 32];
        getrandom::fill(&mut origins_nonce).map_err(|_| invalid_control_io())?;
        super::locks::set_created_file_nonce(&origins.pin_handle()?, &origins_nonce)?;
        let created_directories = if kind == 1 {
            let staging = created.anchor.create_directory(b"staging")?;
            let candidate = created.anchor.create_directory(b"candidate")?;
            let mut staging_nonce = [0; 32];
            let mut candidate_nonce = [0; 32];
            getrandom::fill(&mut staging_nonce).map_err(|_| invalid_control_io())?;
            getrandom::fill(&mut candidate_nonce).map_err(|_| invalid_control_io())?;
            super::locks::set_created_file_nonce(&staging.pin_handle()?, &staging_nonce)?;
            super::locks::set_created_file_nonce(&candidate.pin_handle()?, &candidate_nonce)?;
            Some([
                WorkspaceDirectoryIncarnation {
                    identity: staging.identity(),
                    nonce: staging_nonce,
                },
                WorkspaceDirectoryIncarnation {
                    identity: candidate.identity(),
                    nonce: candidate_nonce,
                },
            ])
        } else {
            None
        };
        let parent_record = if let Some(parent) = parent {
            let registration = parent.registration();
            let descriptor = WorkspaceParentRecord {
                root: registration.root.clone(),
                scope: registration.scope,
                scope_file: registration.scope_file,
            };
            let mut file = created.anchor.create_new(b".workspace.parent.json")?;
            serde_json::to_writer(&mut file, &descriptor)?;
            file.sync_all()?;
            Some(super::locks::file_identity(&file)?)
        } else {
            None
        };
        let scope = WorkspaceBinding {
            root: created.anchor.identity(),
            owner: created.owner,
            registry: created.registry,
            parent_root: created.parent_root,
            parent_coordination: created.parent_coordination,
            parent_cohort: created.parent_cohort,
            publication_binding: created.publication_binding,
            nonce: created.nonce,
            parent_nonce: created.parent_nonce,
            kind,
            writers: writers.identity()?,
            origins: origins.identity(),
            parent_record,
            startup_gate: super::locks::path_identity(&domain.workspace_startup_path())?,
            created_directories,
            origins_nonce,
        };
        let mut domain_file = created.anchor.create_new(WORKSPACE_DOMAIN_NAME)?;
        serde_json::to_writer(&mut domain_file, domain)?;
        domain_file.sync_all()?;
        let mut scope_file = created.anchor.create_new(WORKSPACE_SCOPE_NAME)?;
        scope_file.write_all(&scope.encode())?;
        scope_file.sync_all()?;
        let scope_id = super::locks::file_identity(&scope_file)?;
        created.anchor.sync_namespace()?;
        Ok(Some(Self {
            root: fresh.path().to_owned(),
            gate: domain.workspace_startup_path(),
            scope,
            scope_file: scope_id,
            domain: domain.clone(),
        }))
    }
    /// Reacquire only an existing run registration while borrowing its actual
    /// exclusive owner and the destination's live reference mutation. Stored
    /// scope describes the objects; the normal capability checks supply authority.
    pub(super) fn recover_run(
        root: &std::path::Path,
        domain: &super::maintenance_ownership::ReferenceDomain,
        owner: &super::locks::LockedFile,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        let directory = super::locks::RootedDirectory::open_readonly_root(
            root,
            super::locks::path_identity(root)?,
        )?;
        let file = match directory.open_readonly_file(WORKSPACE_SCOPE_NAME) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let scope = WorkspaceBinding::decode(&read_exact_image::<512>(&file)?)
            .map_err(|_| invalid_control_io())?;
        let registration = Self {
            root: root.to_owned(),
            gate: domain.workspace_startup_path(),
            scope,
            scope_file: super::locks::file_identity(&file)?,
            domain: domain.clone(),
        };
        if scope.kind != 1
            || !domain.revalidate()?
            || super::locks::path_identity(&registration.gate)? != scope.startup_gate
        {
            return Err(invalid_control_io().into());
        }
        let gate = super::locks::open_existing_shared_coordination(&registration.gate)?;
        let capability = WorkspaceOwnerCapability {
            registration: &registration,
            owner,
            mutation,
            gate: &gate,
        };
        use super::maintenance_ownership::PublicationRootAuthority;
        if !capability.revalidate()? {
            return Err(invalid_control_io().into());
        }
        registration.startup_binding().read_control()?;
        drop(gate);
        Ok(Some(registration))
    }
    pub(super) fn terminal_complete(&self) -> Result<bool, crate::error::NativeError> {
        let control = self.startup_binding().read_control()?;
        Ok(matches!(
            control.phase,
            WorkspacePhase::Quiescent | WorkspacePhase::Retiring
        ) && control.disposition != WorkspaceDisposition::None
            && control.registered == control.closed
            && control.source_epoch == control.sealed_source)
    }
    fn revalidate(&self) -> Result<bool, crate::error::NativeError> {
        let directory =
            super::locks::RootedDirectory::open_readonly_root(&self.root, self.scope.root)?;
        let file = directory.open_readonly_file(WORKSPACE_SCOPE_NAME)?;
        Ok(super::locks::file_identity(&file)? == self.scope_file
            && read_exact_image::<512>(&file)? == self.scope.encode()
            && super::locks::file_identity(&directory.open_readonly_file(b"lease.lock")?)?
                == self.scope.owner
            && super::locks::file_identity(&directory.open_readonly_file(BINDING_NAME)?)?
                == self.scope.publication_binding)
    }
    fn update(
        &self,
        owner: &super::locks::LockedFile,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        mode: super::locks::LockMode,
        change: &mut impl FnMut(&mut WorkspaceControl) -> Result<(), crate::error::NativeError>,
    ) -> Result<(), crate::error::NativeError> {
        let Some(gate) = super::locks::try_open_existing_locked(&self.gate, mode)? else {
            return Err(invalid_control_io().into());
        };
        let _writers = if mode == super::locks::LockMode::Exclusive {
            let directory =
                super::locks::RootedDirectory::open_readonly_root(&self.root, self.scope.root)?;
            let file = directory.open_existing(b".workspace-writers.lock")?;
            let Some(lock) =
                super::locks::FixedLockedFile::try_lock(file, super::locks::LockMode::Exclusive)
                    .map_err(|_| invalid_control_io())?
            else {
                return Err(invalid_control_io().into());
            };
            if super::locks::file_identity(lock.file())? != self.scope.writers
                || !lock
                    .matches_fixed_header(&super::locks::encode_coordination_incarnation(
                        self.scope.nonce,
                    ))
                    .map_err(|_| invalid_control_io())?
            {
                return Err(invalid_control_io().into());
            }
            crate::storage::run_workspace::ensure_persisted_cleanup_allowed(&self.root)?;
            crate::storage::containment::ensure_retired_registry_empty(&self.root)
                .map_err(crate::error::NativeError::InvalidInput)?;
            Some(lock)
        } else {
            None
        };
        let mut capability = WorkspaceOwnerCapability {
            registration: self,
            owner,
            mutation,
            gate: &gate,
        };
        let Some(mut publication) = try_begin_publication(&mut capability)? else {
            return Err(invalid_control_io().into());
        };
        let mut payload = [0u8; PAYLOAD_BYTES];
        let len = publication.read_primary_control(&mut payload)?;
        let mut control =
            WorkspaceControl::decode(&payload[..len]).map_err(|_| invalid_control_io())?;
        change(&mut control)?;
        let mut scratch = PrimaryPublicationScratch::new();
        publication.publish_primary_control(&control.encode(), &mut scratch)?;
        Ok(())
    }
    pub(crate) fn source_change(
        &self,
        owner: &super::locks::LockedFile,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        publishing: bool,
        resolved: bool,
    ) -> Result<(), crate::error::NativeError> {
        self.update(
            owner,
            mutation,
            super::locks::LockMode::Shared,
            &mut |control| {
                if matches!(
                    control.phase,
                    WorkspacePhase::OwnerTerminal
                        | WorkspacePhase::Closing
                        | WorkspacePhase::Quiescent
                        | WorkspacePhase::Retiring
                        | WorkspacePhase::Removing
                ) {
                    return Ok(());
                }
                if control.phase == WorkspacePhase::NeedsRecovery && !resolved {
                    return Err(invalid_control_io().into());
                }
                control.source_epoch = control
                    .source_epoch
                    .checked_add(1)
                    .ok_or_else(invalid_control_io)?;
                control.admission_epoch = control
                    .admission_epoch
                    .checked_add(1)
                    .ok_or_else(invalid_control_io)?;
                control.phase = if publishing {
                    WorkspacePhase::NeedsRecovery
                } else {
                    WorkspacePhase::Building
                };
                Ok(())
            },
        )
    }
    /// Primary owner event only: caller first proves actual cleanup barriers,
    /// current publication outcome and ALL root lifecycle containment empty.
    /// Startup Exclusive prevents a fresh or paused launcher crossing closure.
    pub(crate) fn terminal(
        &self,
        owner: &super::locks::LockedFile,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        disposition: WorkspaceDisposition,
    ) -> Result<(), crate::error::NativeError> {
        if disposition == WorkspaceDisposition::None {
            return Err(invalid_control_io().into());
        }
        // Retirement markers can emit creation receipts while startup is still
        // open. The final check inside update is nonmutating under Exclusive;
        // any new unretired lifecycle between checks defers closure safely.
        crate::storage::run_workspace::ensure_persisted_cleanup_allowed(&self.root)?;
        crate::storage::containment::ensure_registry_empty(&self.root)
            .map_err(crate::error::NativeError::InvalidInput)?;
        self.update(
            owner,
            mutation,
            super::locks::LockMode::Exclusive,
            &mut |control| {
                if control.phase == WorkspacePhase::NeedsRecovery {
                    return Err(invalid_control_io().into());
                }
                control.admission_epoch = control
                    .admission_epoch
                    .checked_add(1)
                    .ok_or_else(invalid_control_io)?;
                control.phase = WorkspacePhase::Quiescent;
                control.disposition = disposition;
                control.closed = control.registered;
                control.sealed_source = control.source_epoch;
                validate_metadata_creators(
                    &self.root,
                    self.scope.root,
                    &control.metadata_creators,
                )?;
                control.metadata_seal = workspace_metadata_seal(&self.root, self.scope.root)?;
                Ok(())
            },
        )
    }
    pub(crate) fn startup_binding(&self) -> WorkspaceStartupBinding {
        WorkspaceStartupBinding {
            root: self.root.clone(),
            gate: self.gate.clone(),
            scope: self.scope,
            scope_file: self.scope_file,
        }
    }
    pub(crate) fn domain(&self) -> &super::maintenance_ownership::ReferenceDomain {
        &self.domain
    }
}
struct WorkspaceOwnerCapability<'a, 'mutation> {
    registration: &'a WorkspaceRegistration,
    owner: &'a super::locks::LockedFile,
    mutation: &'a super::maintenance_ownership::ReferenceMutation<'mutation>,
    gate: &'a super::locks::LockedFile,
}
impl super::maintenance_ownership::PublicationRootAuthority for WorkspaceOwnerCapability<'_, '_> {
    fn root(&self) -> &std::path::Path {
        &self.registration.root
    }
    fn root_identity(&self) -> super::locks::FileIdentity {
        self.registration.scope.root
    }
    fn coordination_identity(&self) -> super::locks::FileIdentity {
        self.registration.scope.owner
    }
    fn incarnation(&self) -> [u8; 32] {
        self.registration.scope.nonce
    }
    fn cohort_identity(&self) -> super::locks::FileIdentity {
        self.registration.scope_file
    }
    fn revalidate(&self) -> Result<bool, crate::error::NativeError> {
        let Some(parent) = self.mutation.artifact_root_producer_capability()? else {
            return Ok(false);
        };
        let scope = self.registration.scope;
        Ok(self.registration.revalidate()?
            && self.owner.verifies(
                &self.registration.root.join("lease.lock"),
                super::locks::LockMode::Exclusive,
            )?
            && self.owner.identity()? == scope.owner
            && self.owner.coordination_incarnation()? == Some(scope.nonce)
            && self
                .gate
                .verifies(&self.registration.gate, self.gate.mode())?
            && parent.revalidate()?
            && parent.root_identity() == scope.parent_root
            && parent.coordination_identity() == scope.parent_coordination
            && parent.incarnation() == scope.parent_nonce
            && parent.cohort_identity() == scope.parent_cohort)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorkspaceStartupBinding {
    root: std::path::PathBuf,
    gate: std::path::PathBuf,
    scope: WorkspaceBinding,
    scope_file: super::locks::FileIdentity,
}
pub(crate) struct WorkspaceStartupGuard {
    gate: super::locks::LockedFile,
    binding: WorkspaceStartupBinding,
    _same_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl WorkspaceStartupBinding {
    /// Ordinary launcher discovery only; these serialized facts are not a
    /// capability. The child independently opens and checks its OWN actual gate.
    pub(crate) fn discover(
        workspace: &std::path::Path,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        for root in workspace.ancestors() {
            let path = root.join(std::str::from_utf8(WORKSPACE_SCOPE_NAME).unwrap());
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let bytes = match read_exact_image::<512>(&file) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(None),
            };
            let scope = match WorkspaceBinding::decode(&bytes) {
                Ok(scope) => scope,
                Err(_) => return Ok(None),
            };
            let domain_file = std::fs::File::open(
                root.join(std::str::from_utf8(WORKSPACE_DOMAIN_NAME).unwrap()),
            )?;
            #[cfg(windows)]
            const DOMAIN_BOUND: u64 = 8 * 1024 * 1024;
            #[cfg(not(windows))]
            const DOMAIN_BOUND: u64 = 256 * 1024;
            if domain_file.metadata()?.len() > DOMAIN_BOUND {
                return Ok(None);
            }
            let domain: super::maintenance_ownership::ReferenceDomain =
                serde_json::from_reader(domain_file)?;
            let binding = Self {
                root: root.to_owned(),
                gate: domain.workspace_startup_path(),
                scope,
                scope_file: super::locks::file_identity(&file)?,
            };
            binding.check_building()?;
            return Ok(Some(binding));
        }
        Ok(None)
    }
    fn read_control(&self) -> Result<WorkspaceControl, crate::error::NativeError> {
        let directory =
            super::locks::RootedDirectory::open_readonly_root(&self.root, self.scope.root)?;
        let scope_file = directory.open_readonly_file(WORKSPACE_SCOPE_NAME)?;
        if super::locks::file_identity(&scope_file)? != self.scope_file
            || read_exact_image::<512>(&scope_file)? != self.scope.encode()
            || super::locks::file_identity(&directory.open_readonly_file(b"lease.lock")?)?
                != self.scope.owner
        {
            return Err(invalid_control_io().into());
        }
        let binding_file = directory.open_readonly_file(BINDING_NAME)?;
        if super::locks::file_identity(&binding_file)? != self.scope.publication_binding {
            return Err(invalid_control_io().into());
        }
        let binding = PublisherBinding::decode(&read_exact_image::<512>(&binding_file)?)
            .map_err(|_| invalid_control_io())?;
        if binding.root != self.scope.root
            || binding.coordination != self.scope.owner
            || binding.incarnation != self.scope.nonce
        {
            return Err(invalid_control_io().into());
        }
        let selector_file = directory.open_readonly_file(SELECTOR_NAME)?;
        let selector = read_exact_image::<SELECTOR_BYTES>(&selector_file)?;
        let data0 = directory.open_readonly_file(DATA_NAMES[0])?;
        let data1 = directory.open_readonly_file(DATA_NAMES[1])?;
        if super::locks::file_identity(&selector_file)? != binding.selector
            || super::locks::file_identity(&data0)? != binding.data[0]
            || super::locks::file_identity(&data1)? != binding.data[1]
        {
            return Err(invalid_control_io().into());
        }
        let images = [
            read_exact_image::<SLOT_BYTES>(&data0)?,
            read_exact_image::<SLOT_BYTES>(&data1)?,
        ];
        let commit = decode_commit(&selector[..COMMIT_BYTES], &self.scope.nonce).ok();
        let intent = decode_intent(&selector[COMMIT_BYTES..], &self.scope.nonce).ok();
        let slots = [
            decode_slot(&images[0], &self.scope.nonce).ok(),
            decode_slot(&images[1], &self.scope.nonce).ok(),
        ];
        let RecoveryStep::Committed(commit) = recovery_step(commit, intent, slots) else {
            return Err(invalid_control_io().into());
        };
        let slot = &images[commit.active];
        let len = number(&slot[48..56]) as usize;
        let control =
            WorkspaceControl::decode(&slot[56..56 + len]).map_err(|_| invalid_control_io())?;
        Ok(control)
    }
    fn check_building(&self) -> Result<(), crate::error::NativeError> {
        if self.read_control()?.phase != WorkspacePhase::Building {
            return Err(crate::error::NativeError::InvalidInput(
                "workspace startup is closed".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn acquire(&self) -> Result<WorkspaceStartupGuard, crate::error::NativeError> {
        let gate = super::locks::open_existing_shared_coordination(&self.gate)?;
        if !gate.is_current()? || gate.identity()? != self.scope.startup_gate {
            return Err(invalid_control_io().into());
        }
        self.check_building()?;
        Ok(WorkspaceStartupGuard {
            gate,
            binding: self.clone(),
            _same_thread: std::marker::PhantomData,
        })
    }
}
impl WorkspaceStartupGuard {
    pub(crate) fn recheck_building(&self) -> Result<(), crate::error::NativeError> {
        if !self.gate.is_current()? {
            return Err(invalid_control_io().into());
        }
        self.binding.check_building()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceOutputBinding {
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) coordination: super::locks::FileIdentity,
    pub(crate) registration: super::locks::FileIdentity,
    pub(crate) publication_binding: super::locks::FileIdentity,
    pub(crate) incarnation: [u8; 32],
    pub(crate) registry: super::locks::FileIdentity,
    pub(crate) parent: super::locks::FileIdentity,
    pub(crate) output: super::locks::FileIdentity,
    pub(crate) operation: [u8; 32],
    pub(crate) name: [u8; 255],
    pub(crate) name_len: u8,
}
pub(crate) fn workspace_origin_name(
    incarnation: &[u8; 32],
    parent: super::locks::FileIdentity,
    output: super::locks::FileIdentity,
    operation: &[u8; 32],
) -> [u8; 64] {
    let mut hash = Sha256::new();
    hash.update(incarnation);
    for id in [parent, output] {
        let (a, b) = id.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
    }
    hash.update(operation);
    let digest = hash.finalize();
    let mut name = [0; 64];
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (index, byte) in digest.iter().enumerate() {
        name[index * 2] = HEX[(byte >> 4) as usize];
        name[index * 2 + 1] = HEX[(byte & 15) as usize];
    }
    name
}
/// Actual shared writer-control + live reference mutation, not stored markers.
/// It is retained through completed-origin publication and cannot cross threads.
struct WorkspaceOutputLease {
    binding: WorkspaceStartupBinding,
    origins: super::locks::RootedDirectory,
    mutation: super::maintenance_ownership::ReferenceMutation<'static>,
    writers: super::locks::FixedLockedFile,
}
impl WorkspaceOutputLease {
    fn acquire(path: &std::path::Path) -> Result<Option<Self>, crate::error::NativeError> {
        let Some(binding) = WorkspaceStartupBinding::discover(path.parent().unwrap_or(path))?
        else {
            return Ok(None);
        };
        let startup = binding.acquire()?;
        let directory = super::locks::RootedDirectory::open(&binding.root, binding.scope.root)?;
        let file = directory.open_existing(b".workspace-writers.lock")?;
        let Some(writers) =
            super::locks::FixedLockedFile::try_lock(file, super::locks::LockMode::Shared)
                .map_err(|_| invalid_control_io())?
        else {
            return Err(invalid_control_io().into());
        };
        if super::locks::file_identity(writers.file())? != binding.scope.writers
            || !writers
                .matches_fixed_header(&super::locks::encode_coordination_incarnation(
                    binding.scope.nonce,
                ))
                .map_err(|_| invalid_control_io())?
        {
            return Ok(None);
        }
        let domain_file = directory.open_readonly_file(WORKSPACE_DOMAIN_NAME)?;
        #[cfg(windows)]
        const DOMAIN_BOUND: u64 = 8 * 1024 * 1024;
        #[cfg(not(windows))]
        const DOMAIN_BOUND: u64 = 256 * 1024;
        if domain_file.metadata()?.len() > DOMAIN_BOUND {
            return Ok(None);
        }
        let domain: super::maintenance_ownership::ReferenceDomain =
            serde_json::from_reader(domain_file)?;
        let mutation = super::maintenance_ownership::ReferenceMutation::begin_in_domain(&domain)?;
        let Some(parent) = mutation.artifact_root_producer_capability()? else {
            return Ok(None);
        };
        if parent.root_identity() != binding.scope.parent_root
            || parent.coordination_identity() != binding.scope.parent_coordination
            || parent.incarnation() != binding.scope.parent_nonce
            || parent.cohort_identity() != binding.scope.parent_cohort
        {
            return Ok(None);
        }
        let origins = directory.open_directory(b".workspace-origins")?;
        if origins.identity() != binding.scope.origins
            || super::locks::created_file_nonce(&origins.pin_handle()?)?
                != Some(binding.scope.origins_nonce)
        {
            return Ok(None);
        }
        startup.recheck_building()?;
        drop(startup);
        Ok(Some(Self {
            binding,
            writers,
            origins,
            mutation,
        }))
    }
    fn revalidate(&self) -> Result<(), crate::error::NativeError> {
        self.binding.check_building()?;
        let Some(parent) = self.mutation.artifact_root_producer_capability()? else {
            return Err(invalid_control_io().into());
        };
        if parent.root_identity() != self.binding.scope.parent_root
            || parent.coordination_identity() != self.binding.scope.parent_coordination
            || parent.incarnation() != self.binding.scope.parent_nonce
            || parent.cohort_identity() != self.binding.scope.parent_cohort
            || super::locks::file_identity(self.writers.file())? != self.binding.scope.writers
            || !self
                .writers
                .matches_fixed_header(&super::locks::encode_coordination_incarnation(
                    self.binding.scope.nonce,
                ))
                .map_err(|_| invalid_control_io())?
        {
            return Err(invalid_control_io().into());
        }
        Ok(())
    }
}
/// Genuine O_EXCL-created output. There is no constructor accepting an opened
/// File or a claimed creation bool. Only accepted OS bytes enter the digest.
pub(crate) struct WorkspaceOutputFile {
    file: std::fs::File,
    parent: super::locks::RootedDirectory,
    binding: WorkspaceOutputBinding,
    accepted: Option<u64>,
    hash: Sha256,
    scope: WorkspaceOutputLease,
}
impl WorkspaceOutputFile {
    pub(crate) fn create_new(
        path: &std::path::Path,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        use std::path::Component;
        let Some(scope) = WorkspaceOutputLease::acquire(path)? else {
            return Ok(None);
        };
        let relative = match path.strip_prefix(&scope.binding.root) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let mut components = relative.components().peekable();
        let mut parent =
            super::locks::RootedDirectory::open(&scope.binding.root, scope.binding.scope.root)?;
        let mut basename = None;
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            let Some(name) = name.to_str() else {
                return Ok(None);
            };
            if !name.is_ascii() || name.len() > 255 {
                return Ok(None);
            }
            if components.peek().is_none() {
                basename = Some(name.as_bytes());
                break;
            }
            parent = parent.open_directory(name.as_bytes())?;
        }
        let Some(name) = basename else {
            return Ok(None);
        };
        let file = parent.create_new(name)?;
        let output = super::locks::file_identity(&file)?;
        let mut operation = [0; 32];
        if getrandom::fill(&mut operation).is_err()
            || super::locks::set_created_file_nonce(&file, &operation).is_err()
        {
            return Ok(None);
        }
        let mut encoded = [0; 255];
        encoded[..name.len()].copy_from_slice(name);
        let binding = WorkspaceOutputBinding {
            root: scope.binding.scope.root,
            coordination: scope.binding.scope.owner,
            registration: scope.binding.scope_file,
            publication_binding: scope.binding.scope.publication_binding,
            incarnation: scope.binding.scope.nonce,
            registry: scope.binding.scope.origins,
            parent: parent.identity(),
            output,
            operation,
            name: encoded,
            name_len: name.len() as u8,
        };
        Ok(Some(Self {
            file,
            scope,
            parent,
            binding,
            accepted: Some(0),
            hash: Sha256::new(),
        }))
    }
    pub(crate) fn origin_binding(&self) -> WorkspaceOutputBinding {
        self.binding
    }
    pub(crate) fn create_origin(
        &self,
    ) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        create_workspace_origin(&self.scope, self.binding, true)
    }
    pub(crate) fn finish(self) -> Result<WorkspaceCompletedOutput, crate::error::NativeError> {
        self.file.sync_all()?;
        self.scope.revalidate()?;
        let stamp =
            super::locks::FixedFileStamp::capture(&self.file, super::locks::FixedControlKind::File)
                .map_err(|_| invalid_control_io())?;
        let accepted = self.accepted.ok_or_else(invalid_control_io)?;
        if stamp.identity() != self.binding.output
            || stamp.len() != accepted
            || super::locks::created_file_nonce(&self.file)? != Some(self.binding.operation)
            || super::locks::file_identity(
                &self
                    .parent
                    .open_readonly_file(&self.binding.name[..self.binding.name_len as usize])?,
            )? != self.binding.output
        {
            return Err(invalid_control_io().into());
        }
        let expected: [u8; 32] = self.hash.finalize().into();
        let mut actual = Sha256::new();
        let mut bytes = [0u8; 8192];
        let mut offset = 0u64;
        while offset < accepted {
            let len = (accepted - offset).min(bytes.len() as u64) as usize;
            super::locks::read_fixed_range(&self.file, offset, &mut bytes[..len])?;
            actual.update(&bytes[..len]);
            offset += len as u64;
        }
        stamp
            .recheck(&self.file)
            .map_err(|_| invalid_control_io())?;
        let digest: [u8; 32] = actual.finalize().into();
        if digest != expected {
            return Err(invalid_control_io().into());
        }
        Ok(WorkspaceCompletedOutput {
            file: self.file,
            scope: self.scope,
            parent: self.parent,
            binding: self.binding,
            len: accepted,
            digest,
        })
    }
}
impl std::io::Write for WorkspaceOutputFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.file.write(bytes)?;
        self.hash.update(&bytes[..count]);
        self.accepted = self.accepted.and_then(|old| old.checked_add(count as u64));
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
/// Live guard-backed completed event; no Clone/serde/raw File extraction.
pub(crate) struct WorkspaceCompletedOutput {
    file: std::fs::File,
    parent: super::locks::RootedDirectory,
    binding: WorkspaceOutputBinding,
    len: u64,
    digest: [u8; 32],
    scope: WorkspaceOutputLease,
}
impl WorkspaceCompletedOutput {
    pub(crate) fn origin_binding(&self) -> WorkspaceOutputBinding {
        self.binding
    }
    pub(crate) fn len(&self) -> u64 {
        self.len
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn create_origin(
        &self,
    ) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        create_workspace_origin(&self.scope, self.binding, true)
    }
    pub(crate) fn open_origin(&self) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        self.scope.revalidate()?;
        if super::locks::file_identity(&self.file)? != self.binding.output
            || super::locks::file_identity(
                &self
                    .parent
                    .open_readonly_file(&self.binding.name[..self.binding.name_len as usize])?,
            )? != self.binding.output
            || self.file.metadata()?.len() != self.len
            || super::locks::created_file_nonce(&self.file)? != Some(self.binding.operation)
        {
            return Err(invalid_control_io().into());
        }
        create_workspace_origin(&self.scope, self.binding, false)
    }
}
/// Metadata-only file facade borrows the live output scope, so it cannot escape
/// the actual writer/activity guard or expose a payload File/clone.
pub(crate) struct WorkspaceOriginFile<'a> {
    file: std::fs::File,
    identity: super::locks::FileIdentity,
    name: [u8; 64],
    scope: &'a WorkspaceOutputLease,
}
fn create_workspace_origin(
    scope: &WorkspaceOutputLease,
    binding: WorkspaceOutputBinding,
    create: bool,
) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
    scope.revalidate()?;
    let name = workspace_origin_name(
        &binding.incarnation,
        binding.parent,
        binding.output,
        &binding.operation,
    );
    let file = if create {
        scope.origins.create_new(&name)?
    } else {
        scope.origins.open_existing(&name)?
    };
    let identity = super::locks::file_identity(&file)?;
    let receipt_nonce = workspace_receipt_nonce(
        binding.incarnation,
        binding.parent,
        binding.output,
        binding.operation,
    );
    if create {
        super::locks::set_created_file_nonce(&file, &receipt_nonce)?;
    } else if super::locks::created_file_nonce(&file)? != Some(receipt_nonce) {
        return Err(invalid_control_io().into());
    }
    Ok(WorkspaceOriginFile {
        file,
        identity,
        name,
        scope,
    })
}
impl WorkspaceOriginFile<'_> {
    pub(crate) fn identity(&self) -> super::locks::FileIdentity {
        self.identity
    }
    pub(crate) fn len(&self) -> std::io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }
    pub(crate) fn sync_all(&self) -> std::io::Result<()> {
        self.file.sync_all()
    }
    pub(crate) fn sync_parent(&self) -> std::io::Result<()> {
        if super::locks::file_identity(&self.scope.origins.open_readonly_file(&self.name)?)
            .map_err(|_| invalid_control_io())?
            != self.identity
        {
            return Err(invalid_control_io());
        }
        self.scope.origins.sync_namespace()
    }
}
impl std::io::Read for WorkspaceOriginFile<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl std::io::Write for WorkspaceOriginFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.file.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl std::io::Seek for WorkspaceOriginFile<'_> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(position)
    }
}

/// Advisory local workspace seal. Bulk candidate/reference work remains with
/// the caller; this value owns only fixed controls and pre-reserved paths.
pub(crate) struct PreparedTerminalWorkspace {
    frame: TerminalScopeFrame,
    barrier: Option<TerminalScopeFrame>,
    gate: std::path::PathBuf,
    gate_stamp: super::locks::FixedFileStamp,
    epoch: super::maintenance_admission::PreparedScopedInvalidation,
    scratch: super::locks::FixedControlScratch,
    _memory: super::maintenance_budget::MemoryReservation,
}
struct TerminalScopeFrame {
    root: std::path::PathBuf,
    parent: std::path::PathBuf,
    scope: WorkspaceBinding,
    control: WorkspaceControl,
    paths: [std::path::PathBuf; 11],
    stamps: [Option<super::locks::FixedFileStamp>; 11],
    barrier_directory: Option<TerminalBarrierDirectory>,
    _pin: std::fs::File,
}
// A sealed registry may already have disappeared. Bind that absence to the
// actual containing directory so a newly appeared registry cannot be ignored.
// Fixed inline protocol/lease state: boxing would add a separately accounted allocation.
#[allow(clippy::large_enum_variant)]
enum TerminalBarrierDirectory {
    Present {
        path: std::path::PathBuf,
        identity: super::locks::FileIdentity,
        _pin: std::fs::File,
    },
    Absent {
        parent: std::path::PathBuf,
        identity: super::locks::FileIdentity,
        name: [u8; 255],
        name_len: u8,
        _pin: std::fs::File,
    },
}
fn terminal_invalid() -> super::locks::FixedControlError {
    super::locks::FixedControlError::Invalid(super::locks::FixedControlProblem::InvalidRecord)
}
#[cfg(test)]
fn trace_terminal_preparation(stage: &str, error: &super::locks::FixedControlError) {
    if std::env::var_os("CGRAPH_CLEANUP_TRACE").is_some() {
        eprintln!("cleanup terminal preparation {stage}: {error:?}");
    }
}
fn terminal_io(error: crate::error::NativeError) -> super::locks::FixedControlError {
    match error {
        crate::error::NativeError::Io(error) => error.into(),
        _ => terminal_invalid(),
    }
}
impl TerminalScopeFrame {
    fn prepare(
        root: &std::path::Path,
        scratch: &mut super::locks::FixedControlScratch,
    ) -> Result<Self, super::locks::FixedControlError> {
        use super::locks::{open_fixed_control, FixedControlKind as K, FixedFileStamp};
        let names = [
            ".workspace.binding",
            "lease.lock",
            ".workspace-writers.lock",
            ".publication.binding",
            ".publication.selector",
            ".publication.data0",
            ".publication.data1",
            "cleanup-barrier.json",
            "journal.json",
            ".workspace.domain.json",
            ".workspace.parent.json",
        ];
        let paths = names.map(|name| root.join(name));
        let mut stamps = [None; 11];
        for (index, path) in paths.iter().enumerate() {
            match open_fixed_control(path, K::File, false, scratch) {
                Ok(file) => stamps[index] = Some(FixedFileStamp::capture(&file, K::File)?),
                Err(super::locks::FixedControlError::Io(error))
                    if matches!(index, 7 | 8 | 10)
                        && error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let scope_file = open_fixed_control(&paths[0], K::File, false, scratch)?;
        let scope = WorkspaceBinding::decode(&read_exact_image::<512>(&scope_file)?)
            .map_err(|_| terminal_invalid())?;
        let directory = super::locks::RootedDirectory::open_readonly_root(root, scope.root)?;
        if scope.parent_record != stamps[10].map(|stamp| stamp.identity()) {
            #[cfg(test)]
            trace_terminal_preparation("parent_record", &terminal_invalid());
            return Err(terminal_invalid());
        }
        let barrier_directory = if stamps[7].is_some() {
            let file = open_fixed_control(&paths[7], K::File, false, scratch)?;
            if file.metadata()?.len() > 131072 {
                #[cfg(test)]
                trace_terminal_preparation("barrier_size", &terminal_invalid());
                return Err(terminal_invalid());
            }
            let path: std::path::PathBuf =
                serde_json::from_reader(file).map_err(|_| terminal_invalid())?;
            if !path.is_absolute() || path == root {
                #[cfg(test)]
                trace_terminal_preparation("barrier_path", &terminal_invalid());
                return Err(terminal_invalid());
            }
            match open_fixed_control(&path, K::Directory, false, scratch) {
                Ok(pin) => Some(TerminalBarrierDirectory::Present {
                    identity: FixedFileStamp::capture(&pin, K::Directory)?.identity(),
                    path,
                    _pin: pin,
                }),
                Err(super::locks::FixedControlError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    let parent = path.parent().ok_or_else(terminal_invalid)?;
                    let basename = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(terminal_invalid)?
                        .as_bytes();
                    if basename.is_empty() || basename.len() > 255 {
                        return Err(terminal_invalid());
                    }
                    let pin = open_fixed_control(parent, K::Directory, false, scratch)?;
                    let identity = FixedFileStamp::capture(&pin, K::Directory)?.identity();
                    let directory =
                        super::locks::RootedDirectory::open_readonly_root(parent, identity)?;
                    match directory.open_directory_readonly(basename) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                        Ok(_) => return Err(terminal_invalid()),
                    }
                    let mut name = [0; 255];
                    name[..basename.len()].copy_from_slice(basename);
                    Some(TerminalBarrierDirectory::Absent {
                        parent: parent.to_owned(),
                        identity,
                        name,
                        name_len: basename.len() as u8,
                        _pin: pin,
                    })
                }
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        if stamps[1].unwrap().identity() != scope.owner
            || stamps[2].unwrap().identity() != scope.writers
            || stamps[3].unwrap().identity() != scope.publication_binding
        {
            #[cfg(test)]
            trace_terminal_preparation("scope_control_ids", &terminal_invalid());
            return Err(terminal_invalid());
        }
        for index in [1, 2] {
            let file = open_fixed_control(&paths[index], K::File, false, scratch)?;
            if super::locks::read_coordination_incarnation(&file).map_err(terminal_io)?
                != Some(scope.nonce)
            {
                #[cfg(test)]
                trace_terminal_preparation("scope_coordination", &terminal_invalid());
                return Err(terminal_invalid());
            }
        }
        let parent = root.parent().ok_or_else(terminal_invalid)?;
        if super::locks::path_identity(parent).map_err(terminal_io)? != scope.registry {
            #[cfg(test)]
            trace_terminal_preparation("registry_parent", &terminal_invalid());
            return Err(terminal_invalid());
        }
        let binding_file = open_fixed_control(&paths[3], K::File, false, scratch)?;
        let binding = PublisherBinding::decode(&read_exact_image::<512>(&binding_file)?)
            .map_err(|_| terminal_invalid())?;
        if binding.root != scope.root
            || binding.coordination != scope.owner
            || binding.incarnation != scope.nonce
            || binding.selector != stamps[4].unwrap().identity()
            || binding.data != [stamps[5].unwrap().identity(), stamps[6].unwrap().identity()]
        {
            #[cfg(test)]
            trace_terminal_preparation("publication_binding", &terminal_invalid());
            return Err(terminal_invalid());
        }
        let selector = read_exact_image::<SELECTOR_BYTES>(&open_fixed_control(
            &paths[4],
            K::File,
            false,
            scratch,
        )?)?;
        let images = [
            read_exact_image::<SLOT_BYTES>(&open_fixed_control(
                &paths[5],
                K::File,
                false,
                scratch,
            )?)?,
            read_exact_image::<SLOT_BYTES>(&open_fixed_control(
                &paths[6],
                K::File,
                false,
                scratch,
            )?)?,
        ];
        let commit = decode_commit(&selector[..COMMIT_BYTES], &scope.nonce).ok();
        let intent = decode_intent(&selector[COMMIT_BYTES..], &scope.nonce).ok();
        let slots = [
            decode_slot(&images[0], &scope.nonce).ok(),
            decode_slot(&images[1], &scope.nonce).ok(),
        ];
        let RecoveryStep::Committed(commit) = recovery_step(commit, intent, slots) else {
            #[cfg(test)]
            trace_terminal_preparation("control_commit", &terminal_invalid());
            return Err(terminal_invalid());
        };
        let image = &images[commit.active];
        let len = number(&image[48..56]) as usize;
        let control =
            WorkspaceControl::decode(&image[56..56 + len]).map_err(|_| terminal_invalid())?;
        if !matches!(
            control.phase,
            WorkspacePhase::Quiescent | WorkspacePhase::Retiring
        ) || control.disposition == WorkspaceDisposition::None
            || control.registered != control.closed
            || control.source_epoch != control.sealed_source
        {
            #[cfg(test)]
            trace_terminal_preparation("terminal_control", &terminal_invalid());
            return Err(terminal_invalid());
        }
        validate_metadata_creators(root, scope.root, &control.metadata_creators)
            .map_err(terminal_io)?;
        if workspace_metadata_seal(root, scope.root).map_err(terminal_io)? != control.metadata_seal
        {
            #[cfg(test)]
            trace_terminal_preparation("metadata_seal", &terminal_invalid());
            return Err(terminal_invalid());
        }
        for (path, stamp) in paths.iter().zip(stamps) {
            if let Some(stamp) = stamp {
                stamp.recheck(&open_fixed_control(path, K::File, false, scratch)?)?;
            }
        }
        Ok(Self {
            root: root.to_owned(),
            parent: parent.to_owned(),
            scope,
            control,
            paths,
            stamps,
            barrier_directory,
            _pin: directory.pin_handle()?,
        })
    }
    fn check(
        &self,
        scratch: &mut super::locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), super::locks::FixedControlError> {
        use super::locks::{open_fixed_control, FixedControlKind as K};
        grant.take_entries(2)?;
        if super::locks::RootedDirectory::open_fixed(&self.root, self.scope.root, false, scratch)?
            .identity()
            != self.scope.root
            || super::locks::RootedDirectory::open_fixed(
                &self.parent,
                self.scope.registry,
                false,
                scratch,
            )?
            .identity()
                != self.scope.registry
        {
            return Err(terminal_invalid());
        }
        if let Some(barrier) = &self.barrier_directory {
            grant.take_entry()?;
            match barrier {
                TerminalBarrierDirectory::Present { path, identity, .. } => {
                    super::locks::RootedDirectory::open_fixed(path, *identity, false, scratch)?;
                }
                TerminalBarrierDirectory::Absent {
                    parent,
                    identity,
                    name,
                    name_len,
                    ..
                } => {
                    let directory = super::locks::RootedDirectory::open_fixed(
                        parent, *identity, false, scratch,
                    )?;
                    grant.take_entry()?;
                    match directory.open_directory_readonly(&name[..usize::from(*name_len)]) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.into()),
                        Ok(_) => return Err(terminal_invalid()),
                    }
                }
            }
        }
        for (path, stamp) in self.paths.iter().zip(self.stamps) {
            grant.take_entry()?;
            match (open_fixed_control(path, K::File, false, scratch), stamp) {
                (Ok(file), Some(stamp)) => stamp.recheck(&file)?,
                (Err(super::locks::FixedControlError::Io(error)), None)
                    if error.kind() == std::io::ErrorKind::NotFound => {}
                (Err(error), _) => return Err(error),
                _ => return Err(terminal_invalid()),
            }
        }
        Ok(())
    }
    fn lock(
        &self,
        index: usize,
        scratch: &mut super::locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
    ) -> Result<super::locks::FixedLockedFile, super::locks::FixedControlError> {
        grant.take_entry()?;
        let file = super::locks::open_fixed_control(
            &self.paths[index],
            super::locks::FixedControlKind::File,
            true,
            scratch,
        )?;
        self.stamps[index]
            .ok_or_else(terminal_invalid)?
            .recheck(&file)?;
        super::locks::FixedLockedFile::try_lock(file, super::locks::LockMode::Exclusive)?.ok_or(
            super::locks::FixedControlError::Invalid(super::locks::FixedControlProblem::Busy),
        )
    }
}
/// Prepare only genuine registered, durably terminal roots. A persisted external
/// phase-parent association requires its OWN terminal run proof. External
/// barrier identity is fixed by the real owner closure, never nested Empty.
#[cfg(test)]
#[inline(never)]
pub(crate) fn prepare_terminal_workspace(
    root: &std::path::Path,
    domain: &super::maintenance_ownership::ReferenceDomain,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<PreparedTerminalWorkspace, super::locks::FixedControlError> {
    prepare_terminal_workspace_boxed(root, domain, memory)
        .map(super::maintenance_budget::MemoryBox::into_inner)
}
/// Keep the prepared aggregate out of caller Result/Option stack slots. Every
/// fallible preparation has an ordinary RAII owner; only the final infallible
/// field writes initialize the already charged exact-layout allocation.
#[inline(never)]
pub(crate) fn prepare_terminal_workspace_boxed(
    root: &std::path::Path,
    domain: &super::maintenance_ownership::ReferenceDomain,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<
    super::maintenance_budget::MemoryBox<PreparedTerminalWorkspace>,
    super::locks::FixedControlError,
> {
    let mut prepared =
        super::maintenance_budget::MemoryBox::<PreparedTerminalWorkspace>::new_uninit(memory)
            .map_err(super::locks::FixedControlError::Resource)?;
    let size = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(domain.artifact_root().as_os_str().as_encoded_bytes().len())
        .and_then(|n| n.checked_add(1024))
        .and_then(|n| n.checked_mul(40))
        .and_then(|n| n.checked_add(32768))
        .ok_or(super::locks::FixedControlError::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))?;
    let reservation = memory
        .reserve(size)
        .map_err(super::locks::FixedControlError::Resource)?;
    let mut scratch = super::locks::FixedControlScratch::new(
        memory
            .reserve(super::locks::FixedControlScratch::required_bytes())
            .map_err(super::locks::FixedControlError::Resource)?,
    )?;
    let frame = match TerminalScopeFrame::prepare(root, &mut scratch) {
        Ok(frame) => frame,
        Err(error) => {
            #[cfg(test)]
            trace_terminal_preparation("frame", &error);
            return Err(error);
        }
    };
    let epoch = match super::maintenance_admission::prepare_scoped_invalidation(domain, memory) {
        Ok(epoch) => epoch,
        Err(error) => {
            #[cfg(test)]
            trace_terminal_preparation("epoch", &error);
            return Err(error);
        }
    };
    let (root_id, coordination, nonce, cohort) = epoch.root_facts();
    if (
        frame.scope.parent_root,
        frame.scope.parent_coordination,
        frame.scope.parent_nonce,
        frame.scope.parent_cohort,
    ) != (root_id, coordination, nonce, cohort)
    {
        #[cfg(test)]
        trace_terminal_preparation("parent_artifact_scope", &terminal_invalid());
        return Err(terminal_invalid());
    }
    let barrier = if let Some(parent_record) = frame.scope.parent_record {
        let file = super::locks::open_fixed_control(
            &frame.paths[10],
            super::locks::FixedControlKind::File,
            false,
            &mut scratch,
        )?;
        if super::locks::file_identity(&file).map_err(terminal_io)? != parent_record
            || file.metadata()?.len() > 131072
        {
            #[cfg(test)]
            trace_terminal_preparation("parent_record_file", &terminal_invalid());
            return Err(terminal_invalid());
        }
        let parent: WorkspaceParentRecord =
            serde_json::from_reader(file).map_err(|_| terminal_invalid())?;
        if !parent.root.is_absolute() || parent.root == root {
            #[cfg(test)]
            trace_terminal_preparation("parent_record_path", &terminal_invalid());
            return Err(terminal_invalid());
        }
        let barrier = TerminalScopeFrame::prepare(&parent.root, &mut scratch)?;
        if barrier.scope != parent.scope
            || barrier.stamps[0].map(|s| s.identity()) != Some(parent.scope_file)
            || barrier.scope.parent_record.is_some()
            || barrier.scope.parent_root != frame.scope.parent_root
            || barrier.scope.parent_nonce != frame.scope.parent_nonce
        {
            #[cfg(test)]
            trace_terminal_preparation("parent_scope", &terminal_invalid());
            return Err(terminal_invalid());
        }
        Some(barrier)
    } else {
        None
    };
    let gate = domain.workspace_startup_path();
    let gate_file = super::locks::open_fixed_control(
        &gate,
        super::locks::FixedControlKind::File,
        false,
        &mut scratch,
    )?;
    let gate_stamp =
        super::locks::FixedFileStamp::capture(&gate_file, super::locks::FixedControlKind::File)?;
    if gate_stamp.identity() != frame.scope.startup_gate
        || barrier
            .as_ref()
            .is_some_and(|parent| parent.scope.startup_gate != frame.scope.startup_gate)
    {
        #[cfg(test)]
        trace_terminal_preparation("startup_gate", &terminal_invalid());
        return Err(terminal_invalid());
    }
    let target = prepared.as_mut_ptr();
    // SAFETY: allocation is uniquely owned and has PreparedTerminalWorkspace's
    // exact layout. No fallible operation or user callback follows the first
    // write; every field is moved exactly once before assume_init. Before this
    // block, any failure drops only initialized locals and uninitialized storage.
    unsafe {
        std::ptr::addr_of_mut!((*target).frame).write(frame);
        std::ptr::addr_of_mut!((*target).barrier).write(barrier);
        std::ptr::addr_of_mut!((*target).gate).write(gate);
        std::ptr::addr_of_mut!((*target).gate_stamp).write(gate_stamp);
        std::ptr::addr_of_mut!((*target).epoch).write(epoch);
        std::ptr::addr_of_mut!((*target).scratch).write(scratch);
        std::ptr::addr_of_mut!((*target)._memory).write(reservation);
        Ok(prepared.assume_init())
    }
}
/// Scoped authority exists only while actual gate, owner, writer and selector
/// locks remain held. It cannot be cloned, serialized or moved off the callback.
pub(crate) struct TerminalWorkspaceLease<'a> {
    directory: &'a super::locks::RootedDirectory,
    frame: &'a TerminalScopeFrame,
}
impl TerminalWorkspaceLease<'_> {
    pub(crate) fn directory(&self) -> &super::locks::RootedDirectory {
        self.directory
    }
    // SourceE is the original sealed source, distinct from the current/admission epoch.
    #[allow(clippy::misnamed_getters)]
    pub(crate) fn source_epoch(&self) -> u64 {
        self.frame.control.sealed_source
    }
    pub(crate) fn admission_epoch(&self) -> u64 {
        self.frame.control.admission_epoch
    }
    #[cfg(test)]
    pub(crate) fn root_identity(&self) -> super::locks::FileIdentity {
        self.frame.scope.root
    }
}
pub(crate) fn with_terminal_workspace(
    prepared: &mut PreparedTerminalWorkspace,
    grant: &mut super::maintenance_budget::MutationGrant,
    effect_entries: usize,
    apply: &mut impl FnMut(
        &TerminalWorkspaceLease<'_>,
        &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), super::locks::FixedControlError>,
) -> Result<super::maintenance_admission::EpochAdvance, super::locks::FixedControlError> {
    use super::locks::{open_fixed_control, FixedControlKind as K, FixedLockedFile, LockMode};
    grant.take_entry()?;
    let gate_file = open_fixed_control(&prepared.gate, K::File, true, &mut prepared.scratch)?;
    prepared.gate_stamp.recheck(&gate_file)?;
    let _gate = FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?.ok_or(
        super::locks::FixedControlError::Invalid(super::locks::FixedControlProblem::Busy),
    )?;
    let _owner = prepared.frame.lock(1, &mut prepared.scratch, grant)?;
    let _writers = prepared.frame.lock(2, &mut prepared.scratch, grant)?;
    let _selector = prepared.frame.lock(4, &mut prepared.scratch, grant)?;
    let _barrier_owner = prepared
        .barrier
        .as_ref()
        .map(|frame| frame.lock(1, &mut prepared.scratch, grant))
        .transpose()?;
    let _barrier_writers = prepared
        .barrier
        .as_ref()
        .map(|frame| frame.lock(2, &mut prepared.scratch, grant))
        .transpose()?;
    let _barrier_selector = prepared
        .barrier
        .as_ref()
        .map(|frame| frame.lock(4, &mut prepared.scratch, grant))
        .transpose()?;
    grant.take_entry()?;
    let directory = super::locks::RootedDirectory::open_fixed(
        &prepared.frame.root,
        prepared.frame.scope.root,
        true,
        &mut prepared.scratch,
    )?;
    super::maintenance_admission::with_scoped_reference_invalidation(
        &mut prepared.epoch,
        grant,
        effect_entries,
        &mut |scratch, grant| {
            prepared.frame.check(scratch, grant)?;
            if let Some(barrier) = &prepared.barrier {
                barrier.check(scratch, grant)?;
            }
            Ok(())
        },
        &mut |_, grant| {
            apply(
                &TerminalWorkspaceLease {
                    directory: &directory,
                    frame: &prepared.frame,
                },
                grant,
            )
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceOriginScope {
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) coordination: super::locks::FileIdentity,
    pub(crate) registration: super::locks::FileIdentity,
    pub(crate) publication_binding: super::locks::FileIdentity,
    pub(crate) incarnation: [u8; 32],
    pub(crate) registry: super::locks::FileIdentity,
}
impl TerminalScopeFrame {
    fn origin_scope(&self) -> WorkspaceOriginScope {
        WorkspaceOriginScope {
            root: self.scope.root,
            coordination: self.scope.owner,
            registration: self.stamps[0].unwrap().identity(),
            publication_binding: self.scope.publication_binding,
            incarnation: self.scope.nonce,
            registry: self.scope.origins,
        }
    }
}
impl PreparedTerminalWorkspace {
    pub(crate) fn root(&self) -> &std::path::Path {
        &self.frame.root
    }
    pub(crate) fn origin_scope(&self) -> WorkspaceOriginScope {
        self.frame.origin_scope()
    }
    // SourceE is the original sealed source, distinct from the current/admission epoch.
    #[allow(clippy::misnamed_getters)]
    pub(crate) fn source_epoch(&self) -> u64 {
        self.frame.control.sealed_source
    }
    pub(crate) fn admission_epoch(&self) -> u64 {
        self.frame.control.admission_epoch
    }
}
impl TerminalWorkspaceLease<'_> {
    pub(crate) fn origin_scope(&self) -> WorkspaceOriginScope {
        self.frame.origin_scope()
    }
}

/// Actual O_EXCL mkdir event. Existing directories have no conversion API.
/// The scope guards remain live until its optional Bound-directory receipt is
/// durable; this event never certifies bytes or loader usability.
pub(crate) struct WorkspaceCreatedDirectory {
    directory: super::locks::RootedDirectory,
    parent: super::locks::RootedDirectory,
    binding: WorkspaceOutputBinding,
    scope: WorkspaceOutputLease,
}
impl WorkspaceCreatedDirectory {
    pub(crate) fn from_lifecycle(
        fresh: &crate::process_lifecycle::FreshLifecycleDirectory,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        let path = fresh.path();
        let Some(scope) = WorkspaceOutputLease::acquire(path)? else {
            return Ok(None);
        };
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Ok(None);
        };
        if !name.is_ascii() || name.len() > 255 {
            return Ok(None);
        }
        let parent_path = path.parent().ok_or_else(invalid_control_io)?;
        let parent_id = super::locks::path_identity(parent_path)?;
        let parent = super::locks::RootedDirectory::open(parent_path, parent_id)?;
        let directory = parent.open_directory(name.as_bytes())?;
        if directory.identity() != fresh.directory().identity() {
            return Err(invalid_control_io().into());
        }
        let mut operation = [0; 32];
        if getrandom::fill(&mut operation).is_err()
            || super::locks::set_created_file_nonce(&directory.pin_handle()?, &operation).is_err()
        {
            return Ok(None);
        }
        let mut encoded = [0; 255];
        encoded[..name.len()].copy_from_slice(name.as_bytes());
        let binding = WorkspaceOutputBinding {
            root: scope.binding.scope.root,
            coordination: scope.binding.scope.owner,
            registration: scope.binding.scope_file,
            publication_binding: scope.binding.scope.publication_binding,
            incarnation: scope.binding.scope.nonce,
            registry: scope.binding.scope.origins,
            parent: parent.identity(),
            output: directory.identity(),
            operation,
            name: encoded,
            name_len: name.len() as u8,
        };
        parent.sync_namespace()?;
        Ok(Some(Self {
            directory,
            parent,
            binding,
            scope,
        }))
    }

    pub(crate) fn create_new(
        path: &std::path::Path,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        use std::path::Component;
        let Some(scope) = WorkspaceOutputLease::acquire(path)? else {
            return Ok(None);
        };
        let relative = match path.strip_prefix(&scope.binding.root) {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let mut components = relative.components().peekable();
        let mut parent =
            super::locks::RootedDirectory::open(&scope.binding.root, scope.binding.scope.root)?;
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Ok(None);
            };
            let Some(name) = name.to_str() else {
                return Ok(None);
            };
            if !name.is_ascii() || name.len() > 255 {
                return Ok(None);
            }
            if components.peek().is_some() {
                parent = parent.open_directory(name.as_bytes())?;
                continue;
            }
            let directory = parent.create_directory(name.as_bytes())?;
            let mut operation = [0; 32];
            if getrandom::fill(&mut operation).is_err()
                || super::locks::set_created_file_nonce(&directory.pin_handle()?, &operation)
                    .is_err()
            {
                return Ok(None);
            }
            let mut encoded = [0; 255];
            encoded[..name.len()].copy_from_slice(name.as_bytes());
            let binding = WorkspaceOutputBinding {
                root: scope.binding.scope.root,
                coordination: scope.binding.scope.owner,
                registration: scope.binding.scope_file,
                publication_binding: scope.binding.scope.publication_binding,
                incarnation: scope.binding.scope.nonce,
                registry: scope.binding.scope.origins,
                parent: parent.identity(),
                output: directory.identity(),
                operation,
                name: encoded,
                name_len: name.len() as u8,
            };
            parent.sync_namespace()?;
            return Ok(Some(Self {
                directory,
                parent,
                binding,
                scope,
            }));
        }
        Ok(None)
    }
    pub(crate) fn origin_binding(&self) -> WorkspaceOutputBinding {
        self.binding
    }
    pub(crate) fn create_origin(
        &self,
    ) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        self.scope.revalidate()?;
        if self.directory.identity() != self.binding.output
            || self
                .parent
                .open_directory_readonly(&self.binding.name[..self.binding.name_len as usize])?
                .identity()
                != self.binding.output
            || super::locks::created_file_nonce(&self.directory.pin_handle()?)?
                != Some(self.binding.operation)
        {
            return Err(invalid_control_io().into());
        }
        create_workspace_origin(&self.scope, self.binding, true)
    }
}

/// Retention-only predicate for old producer destructors. Any partial, malformed
/// or inaccessible workspace marker protects residue; this mints NO ownership.
pub(crate) fn workspace_retention_required(path: &std::path::Path) -> bool {
    for (index, ancestor) in path.ancestors().enumerate() {
        if index == 64 {
            return true;
        }
        for name in [
            ".workspace.binding",
            ".workspace.domain.json",
            ".workspace-writers.lock",
        ] {
            match std::fs::symlink_metadata(ancestor.join(name)) {
                Ok(_) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return true,
            }
        }
    }
    false
}

#[derive(Clone, Copy)]
pub(crate) struct WorkspaceRemovalScope {
    pub(crate) kind: u8,
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) parent: super::locks::FileIdentity,
    pub(crate) nonce: [u8; 32],
    pub(crate) startup_gate: super::locks::FileIdentity,
    pub(crate) parent_reference: (
        super::locks::FileIdentity,
        super::locks::FileIdentity,
        [u8; 32],
        super::locks::FileIdentity,
    ),
    pub(crate) directories: [Option<(super::locks::FileIdentity, [u8; 32])>; 3],
}
impl PreparedTerminalWorkspace {
    pub(crate) fn removal_scope(&self) -> WorkspaceRemovalScope {
        self.frame.removal_scope()
    }
}
impl TerminalScopeFrame {
    fn removal_scope(&self) -> WorkspaceRemovalScope {
        let d = self.scope.created_directories;
        WorkspaceRemovalScope {
            kind: self.scope.kind,
            root: self.scope.root,
            parent: self.scope.registry,
            nonce: self.scope.nonce,
            startup_gate: self.scope.startup_gate,
            parent_reference: (
                self.scope.parent_root,
                self.scope.parent_coordination,
                self.scope.parent_nonce,
                self.scope.parent_cohort,
            ),
            directories: [
                Some((self.scope.origins, self.scope.origins_nonce)),
                d.map(|x| (x[0].identity, x[0].nonce)),
                d.map(|x| (x[1].identity, x[1].nonce)),
            ],
        }
    }
}
impl TerminalWorkspaceLease<'_> {
    pub(crate) fn removal_scope(&self) -> WorkspaceRemovalScope {
        self.frame.removal_scope()
    }
}

impl PreparedTerminalWorkspace {
    pub(crate) fn scope_digest(&self) -> [u8; 32] {
        Sha256::digest(self.frame.scope.encode()).into()
    }
}

/// Creation-only metadata control event. The exact temp and intended final
/// basename are both bound BEFORE first write. This is not loader completion.
pub(crate) struct WorkspaceCreatedControl {
    fresh: crate::process_lifecycle::FreshLifecycleFile,
    final_name: [u8; 255],
    final_len: u8,
    binding: WorkspaceOutputBinding,
    accepted: Option<u64>,
    hash: Sha256,
    receipt: std::cell::Cell<Option<super::locks::FileIdentity>>,
    scope: WorkspaceOutputLease,
}
/// Fact minted only after the actual private O_EXCL creator's accepted writes
/// and successful atomic publication; no constructor/serde/Clone escape.
pub(crate) struct WorkspaceControlPublication {
    binding: WorkspaceOutputBinding,
    final_name: [u8; 255],
    final_len: u8,
    len: u64,
    digest: [u8; 32],
    receipt: Option<super::locks::FileIdentity>,
}
type WorkspaceCreationParts = (WorkspaceOutputLease, WorkspaceOutputBinding, [u8; 255], u8);

// The fresh creator token stays inline without an extra unaccounted allocation.
#[allow(clippy::large_enum_variant)]
pub(crate) enum WorkspaceControlFile {
    Plain(crate::process_lifecycle::FreshLifecycleFile),
    Tracked(WorkspaceCreatedControl),
}
impl WorkspaceControlFile {
    pub(crate) fn from_created(
        fresh: crate::process_lifecycle::FreshLifecycleFile,
        final_path: &std::path::Path,
    ) -> Self {
        let prepared = (|| -> Result<Option<WorkspaceCreationParts>, crate::error::NativeError> {
            let path = fresh.path();
            if path.parent() != final_path.parent() {
                return Ok(None);
            }
            let Some(scope) = WorkspaceOutputLease::acquire(path)? else {
                return Ok(None);
            };
            let Some(name) = path.file_name().and_then(|x| x.to_str()) else {
                return Ok(None);
            };
            let Some(final_name) = final_path.file_name().and_then(|x| x.to_str()) else {
                return Ok(None);
            };
            if name.is_empty()
                || name.len() > 255
                || !name.is_ascii()
                || final_name.is_empty()
                || final_name.len() > 255
                || !final_name.is_ascii()
            {
                return Ok(None);
            }
            let mut operation = [0; 32];
            if getrandom::fill(&mut operation).is_err()
                || super::locks::set_created_file_nonce(fresh.file(), &operation).is_err()
            {
                return Ok(None);
            }
            let parent =
                super::locks::path_identity(path.parent().ok_or_else(invalid_control_io)?)?;
            let mut original = [0; 255];
            original[..name.len()].copy_from_slice(name.as_bytes());
            let mut intended = [0; 255];
            intended[..final_name.len()].copy_from_slice(final_name.as_bytes());
            let binding = WorkspaceOutputBinding {
                root: scope.binding.scope.root,
                coordination: scope.binding.scope.owner,
                registration: scope.binding.scope_file,
                publication_binding: scope.binding.scope.publication_binding,
                incarnation: scope.binding.scope.nonce,
                registry: scope.binding.scope.origins,
                parent,
                output: super::locks::file_identity(fresh.file())?,
                operation,
                name: original,
                name_len: name.len() as u8,
            };
            Ok(Some((scope, binding, intended, final_name.len() as u8)))
        })();
        match prepared {
            Ok(Some((scope, binding, final_name, final_len))) => {
                Self::Tracked(WorkspaceCreatedControl {
                    fresh,
                    final_name,
                    final_len,
                    binding,
                    accepted: Some(0),
                    hash: Sha256::new(),
                    receipt: std::cell::Cell::new(None),
                    scope,
                })
            }
            _ => Self::Plain(fresh),
        }
    }
    pub(crate) fn tracking(&self) -> Option<&WorkspaceCreatedControl> {
        match self {
            Self::Tracked(x) => Some(x),
            Self::Plain(_) => None,
        }
    }
    pub(crate) fn publish_recorded(
        self,
        path: &std::path::Path,
    ) -> std::io::Result<Option<WorkspaceControlPublication>> {
        self.finish_control(Some(path))
    }
    pub(crate) fn finish_created(self) -> std::io::Result<()> {
        self.finish_created_recorded().map(|_| ())
    }
    pub(crate) fn finish_created_recorded(
        self,
    ) -> std::io::Result<Option<WorkspaceControlPublication>> {
        self.finish_control(None)
    }
    fn finish_control(
        self,
        destination: Option<&std::path::Path>,
    ) -> std::io::Result<Option<WorkspaceControlPublication>> {
        match self {
            Self::Plain(fresh) => {
                fresh.file().sync_all()?;
                let source = fresh.path().to_owned();
                drop(fresh);
                if let Some(path) = destination {
                    std::fs::rename(source, path)?;
                }
                Ok(None)
            }
            Self::Tracked(control) => {
                let source = control.fresh.path();
                let intended = &control.final_name[..control.final_len as usize];
                let target = destination.unwrap_or(source);
                if target.parent() != source.parent()
                    || target.file_name().map(|x| x.as_encoded_bytes()) != Some(intended)
                {
                    return Err(invalid_control_io());
                }
                control
                    .scope
                    .revalidate()
                    .map_err(|_| invalid_control_io())?;
                let parent_path = source.parent().ok_or_else(invalid_control_io)?;
                let parent = super::locks::RootedDirectory::open_readonly_root(
                    parent_path,
                    control.binding.parent,
                )?;
                let source_name = &control.binding.name[..control.binding.name_len as usize];
                if super::locks::file_identity(&parent.open_metadata_file(source_name)?)
                    .map_err(|_| invalid_control_io())?
                    != control.binding.output
                {
                    return Err(invalid_control_io());
                }
                control.fresh.file().sync_all()?;
                let len = control.accepted;
                let valid = len.is_some_and(|len| {
                    control
                        .fresh
                        .file()
                        .metadata()
                        .is_ok_and(|m| m.len() == len)
                }) && super::locks::created_file_nonce(control.fresh.file())?
                    == Some(control.binding.operation);
                let digest: [u8; 32] = control.hash.clone().finalize().into();
                let fact = valid.then(|| WorkspaceControlPublication {
                    binding: control.binding,
                    final_name: control.final_name,
                    final_len: control.final_len,
                    len: len.unwrap(),
                    digest,
                    receipt: control.receipt.get(),
                });
                let WorkspaceCreatedControl { fresh, scope, .. } = control;
                let source = fresh.path().to_owned();
                drop(fresh);
                if let Some(path) = destination {
                    std::fs::rename(source, path)?;
                }
                drop(scope);
                Ok(fact)
            }
        }
    }
}
impl std::io::Write for WorkspaceControlFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(fresh) => {
                let mut file = fresh.file();
                std::io::Write::write(&mut file, bytes)
            }
            Self::Tracked(control) => {
                let mut file = control.fresh.file();
                let count = std::io::Write::write(&mut file, bytes)?;
                control.hash.update(&bytes[..count]);
                control.accepted = control
                    .accepted
                    .and_then(|len| len.checked_add(count as u64));
                Ok(count)
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = match self {
            Self::Plain(fresh) => fresh.file(),
            Self::Tracked(control) => control.fresh.file(),
        };
        std::io::Write::flush(&mut file)
    }
}
impl WorkspaceCreatedControl {
    pub(crate) fn origin_binding(&self) -> WorkspaceOutputBinding {
        self.binding
    }
    pub(crate) fn intended_name(&self) -> &[u8] {
        &self.final_name[..self.final_len as usize]
    }
    pub(crate) fn create_origin(
        &self,
    ) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        if super::locks::file_identity(self.fresh.file())? != self.binding.output
            || super::locks::created_file_nonce(self.fresh.file())? != Some(self.binding.operation)
        {
            return Err(invalid_control_io().into());
        }
        let origin = create_workspace_origin(&self.scope, self.binding, true)?;
        self.receipt.set(Some(origin.identity()));
        Ok(origin)
    }
}

fn validate_metadata_creators(
    root: &std::path::Path,
    id: super::locks::FileIdentity,
    creators: &[Option<WorkspaceMetadataCreator>; 3],
) -> Result<(), crate::error::NativeError> {
    let directory = super::locks::RootedDirectory::open_readonly_root(root, id)?;
    for (name, creator) in ROOT_METADATA_NAMES.into_iter().zip(creators) {
        match (directory.open_readonly_file(name), creator) {
            (Ok(file), Some(creator)) => {
                let stamp = super::locks::FixedFileStamp::capture(
                    &file,
                    super::locks::FixedControlKind::File,
                )
                .map_err(|_| invalid_control_io())?;
                if stamp.identity() != creator.identity
                    || stamp.len() != creator.len
                    || super::locks::created_file_nonce(&file)? != Some(creator.operation)
                {
                    return Err(invalid_control_io().into());
                }
                let mut hash = Sha256::new();
                let mut bytes = [0; 8192];
                let mut offset = 0;
                while offset < creator.len {
                    let len = (creator.len - offset).min(bytes.len() as u64) as usize;
                    super::locks::read_fixed_range(&file, offset, &mut bytes[..len])?;
                    hash.update(&bytes[..len]);
                    offset += len as u64;
                }
                stamp.recheck(&file).map_err(|_| invalid_control_io())?;
                if <[u8; 32]>::from(hash.finalize()) != creator.digest {
                    return Err(invalid_control_io().into());
                }
            }
            (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => {}
            (Err(error), _) => return Err(error.into()),
            _ => return Err(invalid_control_io().into()),
        }
    }
    Ok(())
}
impl WorkspaceRegistration {
    pub(crate) fn record_metadata(
        &self,
        owner: &super::locks::LockedFile,
        mutation: &super::maintenance_ownership::ReferenceMutation<'_>,
        publication: &WorkspaceControlPublication,
    ) -> Result<(), crate::error::NativeError> {
        if publication.binding.root != self.scope.root
            || publication.binding.parent != self.scope.root
            || publication.binding.registration != self.scope_file
            || publication.binding.incarnation != self.scope.nonce
        {
            return Err(invalid_control_io().into());
        }
        let name = &publication.final_name[..publication.final_len as usize];
        let Some(index) = ROOT_METADATA_NAMES.iter().position(|x| *x == name) else {
            return Ok(());
        };
        self.update(
            owner,
            mutation,
            super::locks::LockMode::Shared,
            &mut |control| {
                if !matches!(
                    control.phase,
                    WorkspacePhase::Building | WorkspacePhase::NeedsRecovery
                ) {
                    return Err(invalid_control_io().into());
                }
                control.metadata_creators[index] = Some(WorkspaceMetadataCreator {
                    identity: publication.binding.output,
                    operation: publication.binding.operation,
                    len: publication.len,
                    digest: publication.digest,
                    receipt: publication.receipt,
                });
                control.source_epoch = control
                    .source_epoch
                    .checked_add(1)
                    .ok_or_else(invalid_control_io)?;
                control.admission_epoch = control
                    .admission_epoch
                    .checked_add(1)
                    .ok_or_else(invalid_control_io)?;
                Ok(())
            },
        )
    }
}

pub(crate) fn workspace_receipt_nonce(
    root: [u8; 32],
    parent: super::locks::FileIdentity,
    output: super::locks::FileIdentity,
    operation: [u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"CGW-RECEIPT-01");
    hash.update(root);
    for id in [parent, output] {
        let (a, b) = id.fixed_words();
        hash.update(a.to_le_bytes());
        hash.update(b.to_le_bytes());
    }
    hash.update(operation);
    hash.finalize().into()
}
#[derive(Clone, Copy)]
pub(crate) struct WorkspaceMetadataProof {
    pub(crate) identity: super::locks::FileIdentity,
    pub(crate) operation: [u8; 32],
    pub(crate) receipt: Option<super::locks::FileIdentity>,
}
impl PreparedTerminalWorkspace {
    pub(crate) fn metadata_proofs(&self) -> [Option<WorkspaceMetadataProof>; 3] {
        self.frame.control.metadata_creators.map(|c| {
            c.map(|c| WorkspaceMetadataProof {
                identity: c.identity,
                operation: c.operation,
                receipt: c.receipt,
            })
        })
    }
}

/// Private hardlink name is bound to the actual random creation operation.
/// This pin is special FIFO metadata, never a second link to a payload file.
pub(crate) fn workspace_fifo_pin_name(binding: &WorkspaceOutputBinding) -> [u8; 69] {
    let mut name = [0; 69];
    name[..5].copy_from_slice(b"fifo-");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, byte) in binding.operation.iter().enumerate() {
        name[5 + i * 2] = HEX[(byte >> 4) as usize];
        name[6 + i * 2] = HEX[(byte & 15) as usize];
    }
    name
}
/// Actual mkfifo event, retaining producer exclusion through durable origin.
/// No conversion from an existing FIFO, serialized record or raw file exists.
#[cfg(unix)]
pub(crate) struct WorkspaceCreatedFifo {
    created: super::locks::CreatedContainmentFifo,
    pin: Option<super::locks::CreatedFifoPin>,
    parent: super::locks::RootedDirectory,
    binding: WorkspaceOutputBinding,
    scope: WorkspaceOutputLease,
}
#[cfg(unix)]
impl WorkspaceCreatedFifo {
    pub(crate) fn create_new(
        path: &std::path::Path,
    ) -> Result<Option<Self>, crate::error::NativeError> {
        if path.file_name().and_then(|x| x.to_str()) != Some("writers.fifo") {
            return Ok(None);
        }
        let Some(scope) = WorkspaceOutputLease::acquire(path)? else {
            return Ok(None);
        };
        let relative = path
            .parent()
            .ok_or_else(invalid_control_io)?
            .strip_prefix(&scope.binding.root)
            .map_err(|_| invalid_control_io())?;
        let mut parent =
            super::locks::RootedDirectory::open(&scope.binding.root, scope.binding.scope.root)?;
        for part in relative.components() {
            let std::path::Component::Normal(name) = part else {
                return Err(invalid_control_io().into());
            };
            parent = parent.open_directory(name.as_encoded_bytes())?;
        }
        let mut operation = [0; 32];
        getrandom::fill(&mut operation).map_err(|_| invalid_control_io())?;
        let created = parent.create_containment_fifo()?;
        let mut name = [0; 255];
        name[..12].copy_from_slice(b"writers.fifo");
        let binding = WorkspaceOutputBinding {
            root: scope.binding.scope.root,
            coordination: scope.binding.scope.owner,
            registration: scope.binding.scope_file,
            publication_binding: scope.binding.scope.publication_binding,
            incarnation: scope.binding.scope.nonce,
            registry: scope.binding.scope.origins,
            parent: parent.identity(),
            output: super::locks::file_identity(created.file())?,
            operation,
            name,
            name_len: 12,
        };
        // Pin/origin tracking is optional cleanup metadata. A pin failure
        // preserves the real FIFO for the primary build; create_origin below
        // refuses to attest it unless both actual names/links validate.
        let pin = parent
            .pin_created_containment_fifo(
                &created,
                &scope.origins,
                &workspace_fifo_pin_name(&binding),
            )
            .ok();
        Ok(Some(Self {
            created,
            pin,
            parent,
            binding,
            scope,
        }))
    }
    pub(crate) fn file(&self) -> &std::fs::File {
        self.created.file()
    }
    pub(crate) fn origin_binding(&self) -> WorkspaceOutputBinding {
        self.binding
    }
    pub(crate) fn create_origin(
        &self,
    ) -> Result<WorkspaceOriginFile<'_>, crate::error::NativeError> {
        self.scope.revalidate()?;
        let pin = self.pin.as_ref().ok_or_else(invalid_control_io)?;
        if pin.identity()? != self.binding.output {
            return Err(invalid_control_io().into());
        }
        self.parent
            .open_pinned_fifo(b"writers.fifo", self.binding.output, 2)?;
        self.scope.origins.open_pinned_fifo(
            &workspace_fifo_pin_name(&self.binding),
            self.binding.output,
            2,
        )?;
        create_workspace_origin(&self.scope, self.binding, true)
    }
}
