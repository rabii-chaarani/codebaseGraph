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
        return Err(invalid_control_io().into());
    }
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut readback[..bytes.len()])?;
    if &readback[..bytes.len()] != bytes {
        return Err(invalid_control_io().into());
    }
    let identity = super::locks::file_identity(&file).map_err(control_native_io)?;
    let named = root.open_existing(name)?;
    if super::locks::file_identity(&named).map_err(control_native_io)? != identity {
        return Err(invalid_control_io().into());
    }
    Ok(identity)
}
fn control_native_io(error: crate::error::NativeError) -> std::io::Error {
    match error {
        crate::error::NativeError::Io(error) => error,
        _ => invalid_control_io().into(),
    }
}
/// The only caller holds genuine fresh-root evidence minted in the actual
/// create_dir success branch. Existing producer/acquisition has no constructor.
/// An error leaves finite unknown names; callers keep primary work optional.
pub(crate) fn initialize_publication_controls(
    fresh: &super::maintenance_ownership::FreshPublicationRoot<'_>,
    scratch: &mut PublicationFactoryScratch,
) -> std::io::Result<PublisherBinding> {
    let root = fresh.anchor();
    let nonce = fresh.incarnation();
    let first =
        encode_slot(&mut scratch.slot, &nonce, 0, b"CGBUILD1").map_err(|_| invalid_control_io())?;
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
    capability: &'a mut super::maintenance_ownership::ArtifactRootProducerCapability<'root>,
    binding: PublisherBinding,
    binding_identity: super::locks::FileIdentity,
    commit: CommitFact,
    usable: bool,
}
impl RootPublicationLease<'_, '_> {
    pub(crate) fn directory(&self) -> &super::locks::RootedDirectory {
        &self.directory
    }
    pub(crate) fn root(&self) -> &std::path::Path {
        self.capability.root()
    }
    pub(crate) fn binding(&self) -> &PublisherBinding {
        &self.binding
    }
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
        return Err(invalid_control_io().into());
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
            _ => crate::error::NativeError::Io(invalid_control_io().into()),
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
        return Err(invalid_control_io().into());
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
