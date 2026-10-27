//! Pure, checksummed facts codecs/reducers. A decoded record or caller-provided
//! observation NEVER grants provenance, lifecycle, publication or deletion
//! authority. No filesystem operations occur in this module.
use sha2::{Digest, Sha256};

#[cfg(test)]
pub(crate) const RECEIPT_BYTES: usize = 304;
#[cfg(test)]
pub(crate) const JOURNAL_BYTES: usize = 336;
#[cfg(test)]
pub(crate) const LEAF_BYTES: usize = 224;
#[cfg(test)]
pub(crate) const FRAMES_PER_SEGMENT: u64 = 16;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodecIssue {
    Corrupt,
    Transition,
    #[cfg(test)]
    Stale,
    #[cfg(test)]
    Overflow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub(crate) enum JournalPhase {
    Staging,
    Complete,
    Detaching,
    Detached,
    Draining,
    Done,
    Invalidated,
}
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub(crate) enum ReplayDisposition {
    PreparationOnly,
    NeedsReprepare,
    NeedsRecovery,
    DetachedObservation,
    Finished,
    Unknown,
}
#[cfg(test)]
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
#[cfg(test)]
pub(crate) struct LeafFacts {
    pub(crate) ordinal: u64,
    pub(crate) address: [u8; 64],
    pub(crate) incarnation: [u8; 32],
    pub(crate) origin_claim: [u8; 32],
    pub(crate) bytes_digest: [u8; 32],
    pub(crate) len: u64,
}
#[cfg(test)]
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
#[cfg(test)]
fn put(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
#[cfg(test)]
fn get(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
#[cfg(test)]
fn seal(b: &mut [u8]) {
    let end = b.len() - 32;
    let digest = Sha256::digest(&b[..end]);
    b[end..].copy_from_slice(&digest);
}
#[cfg(test)]
fn check(b: &[u8], magic: &[u8; 8]) -> Result<(), CodecIssue> {
    let end = b.len() - 32;
    if &b[..8] != magic || b[end..] != Sha256::digest(&b[..end])[..] {
        return Err(CodecIssue::Corrupt);
    }
    Ok(())
}

/// Runtime journal facts stored in the prebound publication stream and slot.
/// The source epoch is immutable; slot generations are independent progress.
pub(crate) const RETIREMENT_FRAME_BYTES: usize = 512;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RetirementStage {
    AppendIntent = 1,
    FrameReady = 2,
    Detached = 3,
    PayloadGone = 4,
    Done = 5,
    ResetIntent = 6,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetirementLeaf {
    pub(crate) stage: RetirementStage,
    pub(crate) source_epoch: u64,
    pub(crate) stage_generation: u64,
    pub(crate) stream_offset: u64,
    pub(crate) key: [u8; 32],
    pub(crate) directory: super::locks::FileIdentity,
    pub(crate) payload: super::locks::FileIdentity,
    pub(crate) receipt: super::locks::FileIdentity,
    pub(crate) scope: super::maintenance_admission::RootOriginBinding,
    pub(crate) operation: [u8; 32],
    pub(crate) byte_len: u64,
    pub(crate) digest: [u8; 32],
    pub(crate) directory_created: bool,
    pub(crate) receipt_digest: [u8; 32],
    pub(crate) source_kind: u8,
    pub(crate) source_name: [u8; 134],
}
impl RetirementLeaf {
    pub(crate) fn encode(&self) -> [u8; RETIREMENT_FRAME_BYTES] {
        let mut b = [0; RETIREMENT_FRAME_BYTES];
        b[..8].copy_from_slice(b"CGLEAF01");
        b[8] = self.stage as u8;
        b[16..24].copy_from_slice(&self.source_epoch.to_le_bytes());
        b[24..32].copy_from_slice(&self.stage_generation.to_le_bytes());
        b[32..40].copy_from_slice(&self.stream_offset.to_le_bytes());
        b[40..72].copy_from_slice(&self.key);
        for (i, id) in [
            self.directory,
            self.payload,
            self.receipt,
            self.scope.root,
            self.scope.coordination,
            self.scope.cohort,
            self.scope.publication_binding,
        ]
        .iter()
        .enumerate()
        {
            let (a, c) = id.fixed_words();
            let o = 72 + i * 16;
            b[o..o + 8].copy_from_slice(&a.to_le_bytes());
            b[o + 8..o + 16].copy_from_slice(&c.to_le_bytes());
        }
        b[184..216].copy_from_slice(&self.scope.incarnation);
        b[216..248].copy_from_slice(&self.operation);
        b[248..256].copy_from_slice(&self.byte_len.to_le_bytes());
        b[256..288].copy_from_slice(&self.digest);
        b[288] = u8::from(self.directory_created);
        b[289..321].copy_from_slice(&self.receipt_digest);
        b[321] = self.source_kind;
        b[322..456].copy_from_slice(&self.source_name);
        let hash = Sha256::digest(&b[..480]);
        b[480..].copy_from_slice(&hash);
        b
    }
    pub(crate) fn decode(b: &[u8; RETIREMENT_FRAME_BYTES]) -> Result<Self, CodecIssue> {
        if &b[..8] != b"CGLEAF01"
            || b[9..16].iter().any(|x| *x != 0)
            || b[456..480].iter().any(|x| *x != 0)
            || b[321] > 1
            || (b[321] == 0 && b[322..456].iter().any(|x| *x != 0))
            || b[288] > 1
            || b[480..] != Sha256::digest(&b[..480])[..]
        {
            return Err(CodecIssue::Corrupt);
        }
        let stage = match b[8] {
            1 => RetirementStage::AppendIntent,
            2 => RetirementStage::FrameReady,
            3 => RetirementStage::Detached,
            4 => RetirementStage::PayloadGone,
            5 => RetirementStage::Done,
            6 => RetirementStage::ResetIntent,
            _ => return Err(CodecIssue::Corrupt),
        };
        let u = |o| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let id = |o| super::locks::FileIdentity::from_fixed_words(u(o), u(o + 8));
        if u(24) != (stage as u64) - 1 {
            return Err(CodecIssue::Transition);
        }
        if b[321] == 1 {
            let name = &b[322..456];
            if &name[..5] != b".tmp-"
                || name[69] != b'-'
                || !name[5..69]
                    .iter()
                    .chain(name[70..].iter())
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
            {
                return Err(CodecIssue::Corrupt);
            }
            let digits = b"0123456789abcdef";
            for (i, key) in b[40..72].iter().enumerate() {
                if name[5 + i * 2] != digits[(key >> 4) as usize]
                    || name[6 + i * 2] != digits[(key & 15) as usize]
                {
                    return Err(CodecIssue::Corrupt);
                }
            }
        }
        Ok(Self {
            stage,
            source_epoch: u(16),
            stage_generation: u(24),
            stream_offset: u(32),
            key: b[40..72].try_into().unwrap(),
            directory: id(72),
            payload: id(88),
            receipt: id(104),
            scope: super::maintenance_admission::RootOriginBinding {
                root: id(120),
                coordination: id(136),
                cohort: id(152),
                publication_binding: id(168),
                incarnation: b[184..216].try_into().unwrap(),
            },
            operation: b[216..248].try_into().unwrap(),
            byte_len: u(248),
            digest: b[256..288].try_into().unwrap(),
            directory_created: b[288] != 0,
            receipt_digest: b[289..321].try_into().unwrap(),
            source_kind: b[321],
            source_name: b[322..456].try_into().unwrap(),
        })
    }
    /// Lossless fixed native component. Random operation identity is checked
    /// together with the exact expected physical leaf, never by name alone.
    pub(crate) fn tombstone(&self) -> [u8; 73] {
        let mut name = [0; 73];
        name[..9].copy_from_slice(b".retired-");
        let digits = b"0123456789abcdef";
        for (i, b) in self.operation.iter().enumerate() {
            name[9 + i * 2] = digits[(b >> 4) as usize];
            name[10 + i * 2] = digits[(b & 15) as usize];
        }
        name
    }
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

#[cfg(test)]
mod runtime_retirement_tests {
    use super::*;
    fn leaf() -> RetirementLeaf {
        let id = super::super::locks::FileIdentity::from_fixed_words(7, 11);
        RetirementLeaf {
            stage: RetirementStage::AppendIntent,
            source_epoch: 91,
            stage_generation: 0,
            stream_offset: 0,
            key: [0xab; 32],
            directory: id,
            payload: id,
            receipt: id,
            scope: super::super::maintenance_admission::RootOriginBinding {
                root: id,
                coordination: id,
                cohort: id,
                publication_binding: id,
                incarnation: [8; 32],
            },
            operation: [9; 32],
            byte_len: 1_000_000,
            digest: [17; 32],
            directory_created: true,
            receipt_digest: [19; 32],
            source_kind: 0,
            source_name: [0; 134],
        }
    }
    #[test]
    fn runtime_leaf_frames_preserve_original_epoch_across_all_progress() {
        let mut l = leaf();
        for stage in [
            RetirementStage::AppendIntent,
            RetirementStage::FrameReady,
            RetirementStage::Detached,
            RetirementStage::PayloadGone,
            RetirementStage::Done,
            RetirementStage::ResetIntent,
        ] {
            l.stage = stage;
            l.stage_generation = (stage as u64) - 1;
            let decoded = RetirementLeaf::decode(&l.encode()).unwrap();
            assert_eq!(decoded, l);
            assert_eq!(decoded.source_epoch, 91);
        }
        l.stage_generation = 0;
        assert_eq!(
            RetirementLeaf::decode(&l.encode()),
            Err(CodecIssue::Transition)
        );
    }
    #[test]
    fn runtime_leaf_temp_address_is_lossless_bound_and_strict() {
        let mut l = leaf();
        l.source_kind = 1;
        l.source_name[..5].copy_from_slice(b".tmp-");
        l.source_name[5..69].copy_from_slice("ab".repeat(32).as_bytes());
        l.source_name[69] = b'-';
        l.source_name[70..].fill(b'7');
        assert_eq!(RetirementLeaf::decode(&l.encode()).unwrap(), l);
        l.source_name[5] = b'c';
        assert_eq!(
            RetirementLeaf::decode(&l.encode()),
            Err(CodecIssue::Corrupt)
        );
    }
    #[test]
    fn runtime_leaf_tombstone_and_corruption_never_alias_new_operation() {
        let l = leaf();
        let mut other = l;
        other.operation[0] ^= 1;
        assert_ne!(l.tombstone(), other.tombstone());
        assert!(l.tombstone().starts_with(b".retired-"));
        let mut bytes = l.encode();
        bytes[72] ^= 1;
        assert_eq!(RetirementLeaf::decode(&bytes), Err(CodecIssue::Corrupt));
    }
}

/// Fixed restore intent facts. Construction/decoding grants no source or
/// destination authority; the backup adapter must rejoin the actual association.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BackupRestoreRecord {
    pub(crate) stage: u8,
    pub(crate) source_epoch: u64,
    pub(crate) root: super::locks::FileIdentity,
    pub(crate) shard: super::locks::FileIdentity,
    pub(crate) source: super::locks::FileIdentity,
    pub(crate) payload: super::locks::FixedFileStamp,
    pub(crate) digest: [u8; 32],
    pub(crate) destination: super::locks::FileIdentity,
    pub(crate) operation: [u8; 32],
    pub(crate) name: [u8; 128],
    pub(crate) name_len: u8,
}
impl BackupRestoreRecord {
    pub(crate) fn encode(&self) -> [u8; 512] {
        use sha2::{Digest, Sha256};
        let mut b = [0; 512];
        b[..8].copy_from_slice(b"CGBKR001");
        b[8] = self.stage;
        b[16..24].copy_from_slice(&self.source_epoch.to_le_bytes());
        for (at, id) in [
            (24, self.root),
            (40, self.shard),
            (56, self.source),
            (184, self.destination),
        ] {
            let (a, c) = id.fixed_words();
            b[at..at + 8].copy_from_slice(&a.to_le_bytes());
            b[at + 8..at + 16].copy_from_slice(&c.to_le_bytes());
        }
        self.payload
            .encode_checkpoint((&mut b[72..152]).try_into().unwrap());
        b[152..184].copy_from_slice(&self.digest);
        b[200..232].copy_from_slice(&self.operation);
        b[232] = self.name_len;
        b[233..361].copy_from_slice(&self.name);
        let hash = Sha256::digest(&b[..480]);
        b[480..].copy_from_slice(&hash);
        b
    }
    pub(crate) fn decode(b: &[u8; 512]) -> Result<Self, CodecIssue> {
        use sha2::{Digest, Sha256};
        let n = usize::from(b[232]);
        if &b[..8] != b"CGBKR001"
            || !(1..=4).contains(&b[8])
            || b[9..16].iter().chain(b[361..480].iter()).any(|x| *x != 0)
            || n == 0
            || n > 128
            || b[233 + n..361].iter().any(|x| *x != 0)
            || super::maintenance_backup::BackupAddress::parse(&b[233..233 + n]).is_none()
            || b[200..232].iter().all(|x| *x == 0)
            || b[480..] != Sha256::digest(&b[..480])[..]
        {
            return Err(CodecIssue::Corrupt);
        }
        let id = |at| {
            super::locks::FileIdentity::from_fixed_words(
                u64::from_le_bytes(b[at..at + 8].try_into().unwrap()),
                u64::from_le_bytes(b[at + 8..at + 16].try_into().unwrap()),
            )
        };
        Ok(Self {
            stage: b[8],
            source_epoch: u64::from_le_bytes(b[16..24].try_into().unwrap()),
            root: id(24),
            shard: id(40),
            source: id(56),
            payload: super::locks::FixedFileStamp::decode_checkpoint(
                b[72..152].try_into().unwrap(),
            )
            .map_err(|_| CodecIssue::Corrupt)?,
            digest: b[152..184].try_into().unwrap(),
            destination: id(184),
            operation: b[200..232].try_into().unwrap(),
            name: b[233..361].try_into().unwrap(),
            name_len: b[232],
        })
    }
}

#[cfg(test)]
mod backup_restore_tests {
    use super::*;
    #[test]
    fn backup_restore_record_preserves_exact_identities_and_rejects_foreign_addresses() {
        let path = std::env::temp_dir().join(format!(
            "dc-backup-codec-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let file = std::fs::File::create(path.join("partition.json")).unwrap();
        let stamp = super::super::locks::FixedFileStamp::capture(
            &file,
            super::super::locks::FixedControlKind::File,
        )
        .unwrap();
        let id = stamp.identity();
        let mut name = [0; 128];
        let address = format!(".replaced-{}-1", "a".repeat(64));
        name[..address.len()].copy_from_slice(address.as_bytes());
        let record = BackupRestoreRecord {
            stage: 1,
            source_epoch: 7,
            root: id,
            shard: id,
            source: id,
            payload: stamp,
            digest: [3; 32],
            destination: id,
            operation: [4; 32],
            name,
            name_len: address.len() as u8,
        };
        assert_eq!(
            BackupRestoreRecord::decode(&record.encode()).unwrap(),
            record
        );
        for stage in [0, 5, 255] {
            let mut invalid = record;
            invalid.stage = stage;
            assert!(BackupRestoreRecord::decode(&invalid.encode()).is_err());
        }
        let mut foreign = record;
        foreign.name[0] = b'x';
        assert!(BackupRestoreRecord::decode(&foreign.encode()).is_err());
        let mut zero_operation = record;
        zero_operation.operation = [0; 32];
        assert!(BackupRestoreRecord::decode(&zero_operation.encode()).is_err());
        drop(file);
        std::fs::remove_dir_all(path).unwrap();
    }
}
