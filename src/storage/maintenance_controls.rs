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
