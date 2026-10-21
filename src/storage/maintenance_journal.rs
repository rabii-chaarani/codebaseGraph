//! Pure, checksummed facts codecs/reducers. A decoded record or caller-provided
//! observation NEVER grants provenance, lifecycle, publication or deletion
//! authority. No filesystem operations occur in this module.
use sha2::{Digest, Sha256};

pub(crate) const RECEIPT_BYTES: usize = 304;
pub(crate) const JOURNAL_BYTES: usize = 336;
pub(crate) const LEAF_BYTES: usize = 224;
pub(crate) const FRAMES_PER_SEGMENT: u64 = 16;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodecIssue {
    Corrupt,
    Transition,
    Stale,
    Overflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ReceiptPhase {
    Intent,
    CreationBound,
    Writing,
    CompleteStaged,
    RenameIntent,
    BackupMoved,
    CanonicalMoved,
    Published,
    RollbackIntent,
    Restored,
}
impl ReceiptPhase {
    fn decode(b: u8) -> Result<Self, CodecIssue> {
        Ok(match b {
            0 => Self::Intent,
            1 => Self::CreationBound,
            2 => Self::Writing,
            3 => Self::CompleteStaged,
            4 => Self::RenameIntent,
            5 => Self::BackupMoved,
            6 => Self::CanonicalMoved,
            7 => Self::Published,
            8 => Self::RollbackIntent,
            9 => Self::Restored,
            _ => return Err(CodecIssue::Corrupt),
        })
    }
}

/// Identity fields are opaque serialization facts awaiting actual guarded OS
/// identity verification. usable_claim is a stored CLAIM, not LoaderUsable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReceiptFacts {
    pub(crate) scope: [u8; 32],
    pub(crate) directory: [u8; 32],
    pub(crate) payload: [u8; 32],
    pub(crate) canonical: [u8; 32],
    pub(crate) backup: [u8; 32],
    pub(crate) record: [u8; 32],
    pub(crate) bytes_digest: [u8; 32],
    pub(crate) bytes_len: u64,
    pub(crate) source_epoch: u64,
    pub(crate) stage_generation: u64,
    pub(crate) phase: ReceiptPhase,
    pub(crate) usable_claim: bool,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum ReceiptEvent {
    BindCreated {
        directory: [u8; 32],
        payload: [u8; 32],
    },
    StartWriting,
    Completed {
        len: u64,
        digest: [u8; 32],
        usable_claim: bool,
    },
    PlanRename {
        canonical: [u8; 32],
        backup: [u8; 32],
    },
    BackupObserved {
        directory: [u8; 32],
    },
    CanonicalObserved {
        directory: [u8; 32],
        payload: [u8; 32],
    },
    PublicationSynced,
    BeginRollback,
    RestoredObserved {
        directory: [u8; 32],
    },
}
impl ReceiptFacts {
    pub(crate) fn reduce(mut self, event: ReceiptEvent) -> Result<Self, CodecIssue> {
        use ReceiptEvent::*;
        use ReceiptPhase::*;
        match (self.phase, event) {
            (Intent, BindCreated { directory, payload })
                if directory != [0; 32] && payload != [0; 32] =>
            {
                self.directory = directory;
                self.payload = payload;
                self.phase = CreationBound;
            }
            (CreationBound, StartWriting) => self.phase = Writing,
            (
                Writing,
                Completed {
                    len,
                    digest,
                    usable_claim,
                },
            ) => {
                self.bytes_len = len;
                self.bytes_digest = digest;
                self.usable_claim = usable_claim;
                self.phase = CompleteStaged;
            }
            (CompleteStaged, PlanRename { canonical, backup }) if canonical != [0; 32] => {
                self.canonical = canonical;
                self.backup = backup;
                self.phase = RenameIntent;
            }
            (RenameIntent, BackupObserved { directory })
                if self.backup != [0; 32] && directory == self.backup =>
            {
                self.phase = BackupMoved
            }
            (RenameIntent | BackupMoved, CanonicalObserved { directory, payload })
                if directory == self.directory && payload == self.payload =>
            {
                self.phase = CanonicalMoved
            }
            (CanonicalMoved, PublicationSynced) => self.phase = Published,
            (RenameIntent | BackupMoved, BeginRollback) => self.phase = RollbackIntent,
            (RollbackIntent, RestoredObserved { directory })
                if self.backup != [0; 32] && directory == self.backup =>
            {
                self.phase = Restored
            }
            _ => return Err(CodecIssue::Transition),
        }
        self.stage_generation = self
            .stage_generation
            .checked_add(1)
            .ok_or(CodecIssue::Overflow)?;
        Ok(self)
    }
    pub(crate) fn encode(&self) -> [u8; RECEIPT_BYTES] {
        let mut b = [0; RECEIPT_BYTES];
        b[..8].copy_from_slice(b"CGOR0001");
        for (i, v) in [
            self.scope,
            self.directory,
            self.payload,
            self.canonical,
            self.backup,
            self.record,
            self.bytes_digest,
        ]
        .iter()
        .enumerate()
        {
            b[8 + i * 32..40 + i * 32].copy_from_slice(v);
        }
        put(&mut b, 232, self.bytes_len);
        put(&mut b, 240, self.source_epoch);
        put(&mut b, 248, self.stage_generation);
        b[256] = self.phase as u8;
        b[257] = u8::from(self.usable_claim);
        seal(&mut b);
        b
    }
    pub(crate) fn decode(b: &[u8; RECEIPT_BYTES]) -> Result<Self, CodecIssue> {
        check(b, b"CGOR0001")?;
        if b[258..272].iter().any(|b| *b != 0) || b[257] > 1 {
            return Err(CodecIssue::Corrupt);
        }
        let a = |i: usize| b[8 + i * 32..40 + i * 32].try_into().unwrap();
        Ok(Self {
            scope: a(0),
            directory: a(1),
            payload: a(2),
            canonical: a(3),
            backup: a(4),
            record: a(5),
            bytes_digest: a(6),
            bytes_len: get(b, 232),
            source_epoch: get(b, 240),
            stage_generation: get(b, 248),
            phase: ReceiptPhase::decode(b[256])?,
            usable_claim: b[257] != 0,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum JournalPhase {
    Staging,
    Complete,
    Detaching,
    Detached,
    Draining,
    Done,
    Invalidated,
}
impl JournalPhase {
    fn decode(b: u8) -> Result<Self, CodecIssue> {
        Ok(match b {
            0 => Self::Staging,
            1 => Self::Complete,
            2 => Self::Detaching,
            3 => Self::Detached,
            4 => Self::Draining,
            5 => Self::Done,
            6 => Self::Invalidated,
            _ => return Err(CodecIssue::Corrupt),
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JournalFacts {
    pub(crate) scope: [u8; 32],
    pub(crate) source_incarnation: [u8; 32],
    pub(crate) private_name: [u8; 32],
    pub(crate) journal_incarnation: [u8; 32],
    pub(crate) frame_chain: [u8; 32],
    pub(crate) next_address: [u8; 64],
    /// Immutable source observation; not relabeled by staging/own invalidation.
    pub(crate) source_epoch: u64,
    pub(crate) stage_generation: u64,
    pub(crate) committed_frames: u64,
    pub(crate) disposed_frames: u64,
    pub(crate) phase: JournalPhase,
    pub(crate) own_epoch_incremented: bool,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum JournalEvent {
    Append {
        ordinal: u64,
        frame_digest: [u8; 32],
        next_address: [u8; 64],
    },
    CompleteObserved {
        frames: u64,
        source_epoch: u64,
    },
    SourceChanged,
    OwnEpochAdvanced {
        before: u64,
        after: u64,
    },
    DetachingRecorded,
    PrivateObserved {
        actual_incarnation: [u8; 32],
    },
    Disposed {
        ordinal: u64,
    },
    Finished {
        private_absent: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplayDisposition {
    PreparationOnly,
    NeedsReprepare,
    NeedsRecovery,
    DetachedObservation,
    Finished,
    Unknown,
}
impl JournalFacts {
    pub(crate) fn reduce(mut self, event: JournalEvent) -> Result<Self, CodecIssue> {
        use JournalEvent::*;
        use JournalPhase::*;
        if let Append { ordinal, .. } = event {
            if ordinal != self.committed_frames {
                return Err(CodecIssue::Stale);
            }
        }
        match (self.phase, event) {
            (
                Staging,
                Append {
                    ordinal,
                    frame_digest,
                    next_address,
                },
            ) if ordinal == self.committed_frames => {
                let mut h = Sha256::new();
                h.update(self.frame_chain);
                h.update(ordinal.to_le_bytes());
                h.update(frame_digest);
                self.frame_chain = h.finalize().into();
                self.committed_frames = self
                    .committed_frames
                    .checked_add(1)
                    .ok_or(CodecIssue::Overflow)?;
                self.next_address = next_address;
            }
            (
                Staging,
                CompleteObserved {
                    frames,
                    source_epoch,
                },
            ) if frames == self.committed_frames && source_epoch == self.source_epoch => {
                self.phase = Complete
            }
            (Staging | Complete, SourceChanged) => self.phase = Invalidated,
            (Complete, OwnEpochAdvanced { before, after })
                if !self.own_epoch_incremented
                    && before == self.source_epoch
                    && before.checked_add(1) == Some(after) =>
            {
                self.own_epoch_incremented = true
            }
            (Complete, DetachingRecorded) if self.own_epoch_incremented => self.phase = Detaching,
            (Detaching, PrivateObserved { actual_incarnation })
                if actual_incarnation == self.source_incarnation
                    && actual_incarnation != [0; 32] =>
            {
                self.phase = Detached
            }
            (Detached | Draining, Disposed { ordinal })
                if ordinal == self.disposed_frames && ordinal < self.committed_frames =>
            {
                self.disposed_frames = self
                    .disposed_frames
                    .checked_add(1)
                    .ok_or(CodecIssue::Overflow)?;
                self.phase = Draining;
            }
            (
                Detached | Draining,
                Finished {
                    private_absent: true,
                },
            ) if self.disposed_frames == self.committed_frames => self.phase = Done,
            _ => return Err(CodecIssue::Transition),
        }
        self.stage_generation = self
            .stage_generation
            .checked_add(1)
            .ok_or(CodecIssue::Overflow)?;
        Ok(self)
    }
    /// Even an own E increment + Detaching record cannot manufacture detached
    /// state when no matching actual private incarnation is observed.
    pub(crate) fn replay_disposition(
        &self,
        private_incarnation: Option<[u8; 32]>,
    ) -> ReplayDisposition {
        use JournalPhase::*;
        match self.phase {
            Staging => ReplayDisposition::PreparationOnly,
            Invalidated => ReplayDisposition::NeedsReprepare,
            Complete if self.own_epoch_incremented => ReplayDisposition::NeedsReprepare,
            Complete => ReplayDisposition::PreparationOnly,
            Detaching | Detached | Draining
                if private_incarnation == Some(self.source_incarnation)
                    && self.source_incarnation != [0; 32] =>
            {
                ReplayDisposition::DetachedObservation
            }
            Detaching => ReplayDisposition::NeedsRecovery,
            Detached | Draining => ReplayDisposition::Unknown,
            Done if private_incarnation.is_none() => ReplayDisposition::Finished,
            Done => ReplayDisposition::Unknown,
        }
    }
    pub(crate) fn encode(&self) -> [u8; JOURNAL_BYTES] {
        let mut b = [0; JOURNAL_BYTES];
        b[..8].copy_from_slice(b"CGJR0001");
        for (i, v) in [
            self.scope,
            self.source_incarnation,
            self.private_name,
            self.journal_incarnation,
            self.frame_chain,
        ]
        .iter()
        .enumerate()
        {
            b[8 + i * 32..40 + i * 32].copy_from_slice(v);
        }
        b[168..232].copy_from_slice(&self.next_address);
        for (o, v) in [
            (232, self.source_epoch),
            (240, self.stage_generation),
            (248, self.committed_frames),
            (256, self.disposed_frames),
        ] {
            put(&mut b, o, v);
        }
        b[264] = self.phase as u8;
        b[265] = u8::from(self.own_epoch_incremented);
        seal(&mut b);
        b
    }
    pub(crate) fn decode(b: &[u8; JOURNAL_BYTES]) -> Result<Self, CodecIssue> {
        check(b, b"CGJR0001")?;
        if b[266..304].iter().any(|b| *b != 0) || b[265] > 1 {
            return Err(CodecIssue::Corrupt);
        }
        let a = |i: usize| b[8 + i * 32..40 + i * 32].try_into().unwrap();
        let result = Self {
            scope: a(0),
            source_incarnation: a(1),
            private_name: a(2),
            journal_incarnation: a(3),
            frame_chain: a(4),
            next_address: b[168..232].try_into().unwrap(),
            source_epoch: get(b, 232),
            stage_generation: get(b, 240),
            committed_frames: get(b, 248),
            disposed_frames: get(b, 256),
            phase: JournalPhase::decode(b[264])?,
            own_epoch_incremented: b[265] != 0,
        };
        if result.disposed_frames > result.committed_frames {
            return Err(CodecIssue::Corrupt);
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeafFacts {
    pub(crate) ordinal: u64,
    pub(crate) address: [u8; 64],
    pub(crate) incarnation: [u8; 32],
    pub(crate) origin_claim: [u8; 32],
    pub(crate) bytes_digest: [u8; 32],
    pub(crate) len: u64,
}
impl LeafFacts {
    pub(crate) fn encode(&self) -> [u8; LEAF_BYTES] {
        let mut b = [0; LEAF_BYTES];
        b[..8].copy_from_slice(b"CGLF0001");
        put(&mut b, 8, self.ordinal);
        b[16..80].copy_from_slice(&self.address);
        b[80..112].copy_from_slice(&self.incarnation);
        b[112..144].copy_from_slice(&self.origin_claim);
        b[144..176].copy_from_slice(&self.bytes_digest);
        put(&mut b, 176, self.len);
        seal(&mut b);
        b
    }
    pub(crate) fn decode(b: &[u8; LEAF_BYTES]) -> Result<Self, CodecIssue> {
        check(b, b"CGLF0001")?;
        if b[184..192].iter().any(|b| *b != 0) {
            return Err(CodecIssue::Corrupt);
        }
        Ok(Self {
            ordinal: get(b, 8),
            address: b[16..80].try_into().unwrap(),
            incarnation: b[80..112].try_into().unwrap(),
            origin_claim: b[112..144].try_into().unwrap(),
            bytes_digest: b[144..176].try_into().unwrap(),
            len: get(b, 176),
        })
    }
    pub(crate) fn segment(&self) -> u64 {
        self.ordinal / FRAMES_PER_SEGMENT
    }
}
fn put(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
fn get(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let digest = Sha256::digest(&b[..end]);
    b[end..].copy_from_slice(&digest);
}
fn check(b: &[u8], magic: &[u8; 8]) -> Result<(), CodecIssue> {
    let end = b.len() - 32;
    if &b[..8] != magic || b[end..] != Sha256::digest(&b[..end])[..] {
        return Err(CodecIssue::Corrupt);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt() -> ReceiptFacts {
        ReceiptFacts {
            scope: [1; 32],
            directory: [0; 32],
            payload: [0; 32],
            canonical: [0; 32],
            backup: [0; 32],
            record: [2; 32],
            bytes_digest: [0; 32],
            bytes_len: 0,
            source_epoch: 23,
            stage_generation: 0,
            phase: ReceiptPhase::Intent,
            usable_claim: false,
        }
    }
    fn journal() -> JournalFacts {
        JournalFacts {
            scope: [1; 32],
            source_incarnation: [2; 32],
            private_name: [3; 32],
            journal_incarnation: [4; 32],
            frame_chain: [0; 32],
            next_address: [0; 64],
            source_epoch: 23,
            stage_generation: 0,
            committed_frames: 0,
            disposed_frames: 0,
            phase: JournalPhase::Staging,
            own_epoch_incremented: false,
        }
    }
    #[test]
    fn dc_receipt_crash_states_never_publish_intent_or_origin_only() {
        let mut r = receipt();
        assert!(r.reduce(ReceiptEvent::PublicationSynced).is_err());
        r = r
            .reduce(ReceiptEvent::BindCreated {
                directory: [5; 32],
                payload: [6; 32],
            })
            .unwrap();
        r = r.reduce(ReceiptEvent::StartWriting).unwrap();
        r = r
            .reduce(ReceiptEvent::Completed {
                len: 900,
                digest: [7; 32],
                usable_claim: false,
            })
            .unwrap();
        assert!(!r.usable_claim);
        r = r
            .reduce(ReceiptEvent::PlanRename {
                canonical: [8; 32],
                backup: [9; 32],
            })
            .unwrap();
        assert!(r
            .reduce(ReceiptEvent::CanonicalObserved {
                directory: [99; 32],
                payload: [6; 32]
            })
            .is_err());
        r = r
            .reduce(ReceiptEvent::BackupObserved { directory: [9; 32] })
            .unwrap();
        let rollback = r
            .reduce(ReceiptEvent::BeginRollback)
            .unwrap()
            .reduce(ReceiptEvent::RestoredObserved { directory: [9; 32] })
            .unwrap();
        assert_eq!(rollback.phase, ReceiptPhase::Restored);
        r = r
            .reduce(ReceiptEvent::CanonicalObserved {
                directory: [5; 32],
                payload: [6; 32],
            })
            .unwrap();
        assert_eq!(r.phase, ReceiptPhase::CanonicalMoved);
        r = r.reduce(ReceiptEvent::PublicationSynced).unwrap();
        assert_eq!(r.phase, ReceiptPhase::Published);
        assert!(!r.usable_claim);
        assert_eq!(r.source_epoch, 23);
        assert_eq!(ReceiptFacts::decode(&r.encode()).unwrap(), r);
    }
    #[test]
    fn dc_journal_101_frames_restart_immutable_source_epoch() {
        let mut j = journal();
        for ordinal in 0..257 {
            let leaf = LeafFacts {
                ordinal,
                address: [ordinal as u8; 64],
                incarnation: [5; 32],
                origin_claim: [6; 32],
                bytes_digest: [7; 32],
                len: 900,
            };
            assert_eq!(LeafFacts::decode(&leaf.encode()).unwrap(), leaf);
            assert_eq!(leaf.segment(), ordinal / 16);
            j = j
                .reduce(JournalEvent::Append {
                    ordinal,
                    frame_digest: Sha256::digest(leaf.encode()).into(),
                    next_address: leaf.address,
                })
                .unwrap();
            j = JournalFacts::decode(&j.encode()).unwrap();
            assert_eq!(j.source_epoch, 23);
            assert_eq!(j.committed_frames, ordinal + 1);
        }
        assert!(j
            .reduce(JournalEvent::Append {
                ordinal: 0,
                frame_digest: [0; 32],
                next_address: [0; 64]
            })
            .is_err());
        j = j
            .reduce(JournalEvent::CompleteObserved {
                frames: 257,
                source_epoch: 23,
            })
            .unwrap();
        j = j
            .reduce(JournalEvent::OwnEpochAdvanced {
                before: 23,
                after: 24,
            })
            .unwrap();
        assert_eq!(j.source_epoch, 23);
        assert_eq!(
            j.replay_disposition(None),
            ReplayDisposition::NeedsReprepare
        );
        assert_eq!(
            j.replay_disposition(Some([2; 32])),
            ReplayDisposition::NeedsReprepare
        );
        j = j.reduce(JournalEvent::DetachingRecorded).unwrap();
        assert_eq!(j.replay_disposition(None), ReplayDisposition::NeedsRecovery);
        assert!(j
            .reduce(JournalEvent::PrivateObserved {
                actual_incarnation: [99; 32]
            })
            .is_err());
        j = j
            .reduce(JournalEvent::PrivateObserved {
                actual_incarnation: [2; 32],
            })
            .unwrap();
        for ordinal in 0..257 {
            j = j.reduce(JournalEvent::Disposed { ordinal }).unwrap();
            j = JournalFacts::decode(&j.encode()).unwrap();
        }
        assert_eq!(
            j.reduce(JournalEvent::Finished {
                private_absent: true
            })
            .unwrap()
            .phase,
            JournalPhase::Done
        );
    }
    #[test]
    fn dc_codecs_reject_torn_corrupt_and_reordered_progress() {
        let mut r = receipt().encode();
        r[30] ^= 1;
        assert_eq!(ReceiptFacts::decode(&r), Err(CodecIssue::Corrupt));
        let mut j = journal();
        assert!(j
            .reduce(JournalEvent::CompleteObserved {
                frames: 1,
                source_epoch: 23
            })
            .is_err());
        j = j.reduce(JournalEvent::SourceChanged).unwrap();
        assert_eq!(j.source_epoch, 23);
        assert!(j
            .reduce(JournalEvent::Append {
                ordinal: 0,
                frame_digest: [0; 32],
                next_address: [0; 64]
            })
            .is_err());
        let mut encoded = j.encode();
        encoded[248] ^= 1;
        assert_eq!(JournalFacts::decode(&encoded), Err(CodecIssue::Corrupt));
    }
}
