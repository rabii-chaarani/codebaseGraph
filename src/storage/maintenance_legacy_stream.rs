//! Bounded transport over the existing prebound maintenance stream. Frames and
//! checkpoints are facts; the admission facade supplies actual write authority.
use super::inventory::MaintenanceTarget;
use super::locks::{FixedControlError, FixedControlProblem};
use super::maintenance_admission::RootOriginBinding;
use super::maintenance_admission::{
    self as admission, PreparedControlUpdate, PreparedPrivateMetadata,
};
use super::maintenance_budget::{CleanupMemory, MemoryReservation, MutationGrant, ReservedBytes};
use super::maintenance_legacy_association::{
    LegacyCacheCertificate, LegacyCheckpoint, LegacyIssue, ReplayLegacyCertificates,
};
use super::maintenance_legacy_bootstrap::LegacyClosedVersion;
use super::maintenance_legacy_staging::{
    ReplayStagingCertificates, StagingIssue, StagingPayloadCertificate,
    StagingProjectionCheckpoint, StagingReplayEvent, StagingRowWitness,
};
use sha2::{Digest, Sha256};
pub(crate) const CURSOR_BYTES: usize = 1105;
pub(crate) const FRAME_LIMIT: usize = 4096;
const HEADER_BYTES: usize = 2048;
const FRAME_PREFIX: usize = 96;
const FRAME_FOOTER: usize = 32;
fn invalid() -> FixedControlError {
    FixedControlError::Invalid(FixedControlProblem::InvalidRecord)
}
fn put(b: &mut [u8], v: u64) {
    b[..8].copy_from_slice(&v.to_le_bytes());
}
fn num(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}
fn finish(b: &mut [u8]) {
    let end = b.len() - 32;
    let d = Sha256::digest(&b[..end]);
    b[end..].copy_from_slice(&d);
}
fn checked(b: &[u8]) -> bool {
    b.len() >= 32 && Sha256::digest(&b[..b.len() - 32])[..] == b[b.len() - 32..]
}
#[derive(Clone, Copy)]
pub(crate) struct ActiveSourceSnapshot {
    database: super::locks::FixedFileStamp,
    manifest: super::locks::FixedFileStamp,
    cache: super::locks::FileIdentity,
    epoch: [u8; 32],
}
impl ActiveSourceSnapshot {
    pub(crate) fn observe(
        database: &std::fs::File,
        manifest: &std::fs::File,
        cache: &super::locks::RootedDirectory,
        domain: &super::maintenance_ownership::ReferenceDomain,
        expected: &super::maintenance_ownership::ReferenceEpoch,
        memory: &CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let _floor = memory.reserve(4096).map_err(FixedControlError::Resource)?;
        if !super::maintenance_ownership::expected_reference_current(domain, expected, memory)? {
            return Err(invalid());
        }
        let database_stamp =
            super::locks::FixedFileStamp::capture(database, super::locks::FixedControlKind::File)?;
        let manifest_stamp =
            super::locks::FixedFileStamp::capture(manifest, super::locks::FixedControlKind::File)?;
        let mut epoch = [0; 512];
        expected.encode_checkpoint(&mut epoch);
        database_stamp.recheck(database)?;
        manifest_stamp.recheck(manifest)?;
        if !super::maintenance_ownership::expected_reference_current(domain, expected, memory)? {
            return Err(invalid());
        }
        Ok(Self {
            database: database_stamp,
            manifest: manifest_stamp,
            cache: cache.identity(),
            epoch: Sha256::digest(epoch).into(),
        })
    }
    fn encode(&self, out: &mut [u8; 512]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLGAS01");
        self.database
            .encode_checkpoint((&mut out[16..96]).try_into().unwrap());
        self.manifest
            .encode_checkpoint((&mut out[96..176]).try_into().unwrap());
        let (a, b) = self.cache.fixed_words();
        put(&mut out[176..], a);
        put(&mut out[184..], b);
        out[192..224].copy_from_slice(&self.epoch);
        finish(out);
    }
    fn decode(b: &[u8; 512]) -> Result<Self, FixedControlError> {
        if &b[..8] != b"CGLGAS01"
            || !checked(b)
            || b[8..16].iter().chain(b[224..480].iter()).any(|b| *b != 0)
        {
            return Err(invalid());
        }
        Ok(Self {
            database: super::locks::FixedFileStamp::decode_checkpoint(
                b[16..96].try_into().unwrap(),
            )?,
            manifest: super::locks::FixedFileStamp::decode_checkpoint(
                b[96..176].try_into().unwrap(),
            )?,
            cache: super::locks::FileIdentity::from_fixed_words(num(&b[176..]), num(&b[184..])),
            epoch: b[192..224].try_into().unwrap(),
        })
    }
    pub(crate) fn context_digest(&self) -> [u8; 32] {
        let mut image = [0; 512];
        self.encode(&mut image);
        Sha256::digest(image).into()
    }
    fn matches(&self, cp: &LegacyCheckpoint) -> bool {
        let mut epoch = [0; 512];
        cp.source_epoch().encode_checkpoint(&mut epoch);
        cp.matches_closed_source(Some(self.database), Some(self.manifest), self.cache)
            && <[u8; 32]>::from(Sha256::digest(epoch)) == self.epoch
    }
}
#[derive(Clone, Copy)]
enum StreamSource {
    Closed(LegacyClosedVersion),
    Active(ActiveSourceSnapshot),
}
impl StreamSource {
    fn encode(&self, out: &mut [u8; 512]) {
        match self {
            Self::Closed(s) => s.encode_checkpoint(out),
            Self::Active(s) => s.encode(out),
        }
    }
    fn decode(b: &[u8; 512]) -> Result<Self, FixedControlError> {
        if &b[..8] == b"CGLGAS01" {
            Ok(Self::Active(ActiveSourceSnapshot::decode(b)?))
        } else {
            Ok(Self::Closed(LegacyClosedVersion::decode_checkpoint(b)?))
        }
    }
    fn digest(&self) -> [u8; 32] {
        match self {
            Self::Closed(s) => s.context_digest(),
            Self::Active(s) => s.context_digest(),
        }
    }
    fn matches(&self, cp: &LegacyCheckpoint) -> bool {
        match self {
            Self::Closed(s) => {
                cp.matches_closed_source(s.database_stamp(), s.manifest_stamp(), s.cache_identity())
            }
            Self::Active(s) => s.matches(cp),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyStreamKind {
    Association,
    Staging,
    Backup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyFrameKind {
    CacheCertificate,
    AssociationCheckpoint,
    StagingRows,
    StagingPayload,
    StagingCheckpoint,
    StagingSeal,
    NativeCohort,
}
impl LegacyFrameKind {
    fn byte(self) -> u8 {
        match self {
            Self::CacheCertificate => 1,
            Self::AssociationCheckpoint => 2,
            Self::StagingRows => 3,
            Self::StagingPayload => 4,
            Self::StagingCheckpoint => 5,
            Self::StagingSeal => 6,
            Self::NativeCohort => 7,
        }
    }
    fn parse(v: u8) -> Result<Self, FixedControlError> {
        match v {
            1 => Ok(Self::CacheCertificate),
            2 => Ok(Self::AssociationCheckpoint),
            3 => Ok(Self::StagingRows),
            4 => Ok(Self::StagingPayload),
            5 => Ok(Self::StagingCheckpoint),
            6 => Ok(Self::StagingSeal),
            7 => Ok(Self::NativeCohort),
            _ => Err(invalid()),
        }
    }
    fn checkpoint(self) -> bool {
        matches!(
            self,
            Self::AssociationCheckpoint
                | Self::StagingCheckpoint
                | Self::StagingSeal
                | Self::NativeCohort
        )
    }
    fn validate(self, p: &[u8]) -> Result<(), FixedControlError> {
        match self {
            Self::CacheCertificate => {
                LegacyCacheCertificate::decode(p.try_into().map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            }
            Self::AssociationCheckpoint => {
                LegacyCheckpoint::decode(p.try_into().map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            }
            Self::StagingRows => {
                if p.is_empty() || p.len() % 768 != 0 || p.len() > 3072 {
                    return Err(invalid());
                }
                for row in p.chunks_exact(768) {
                    StagingRowWitness::decode(row.try_into().unwrap()).map_err(|_| invalid())?;
                }
            }
            Self::StagingPayload => {
                StagingPayloadCertificate::decode(p.try_into().map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            }
            Self::NativeCohort => {
                if p.len() != 3968 || &p[..8] != b"CGLGNCP1" || !checked(p) {
                    return Err(invalid());
                }
                let original = LegacyStreamHeader::decode(&p[8..2056])?;
                let retiring =
                    LegacyClosedVersion::decode_checkpoint(p[2056..2568].try_into().unwrap())?;
                if !original.done()
                    || original.kind() != LegacyStreamKind::Association
                    || retiring.phase() != super::maintenance_legacy_guard::LegacyPhase::Retiring
                    || !retiring.same_job_source(&original.source_version().ok_or_else(invalid)?)
                {
                    return Err(invalid());
                }
            }
            Self::StagingSeal => {
                super::maintenance_legacy_staging::StagingAssociation::decode(
                    p.try_into().map_err(|_| invalid())?,
                )
                .map_err(|_| invalid())?;
            }
            Self::StagingCheckpoint => {
                StagingProjectionCheckpoint::decode(p.try_into().map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    AwaitCheckpoint,
    AppendIntent,
    Done,
    ResetIntent,
}
/// Copy fixed facts. Decode never acquires a source or destination capability.
#[derive(Clone, Copy)]
pub(crate) struct LegacyStreamHeader {
    cursor: [u8; CURSOR_BYTES],
    source: StreamSource,
    context: [u8; 32],
    kind: LegacyStreamKind,
    phase: Phase,
    physical_end: u64,
    committed_end: u64,
    sequence: u64,
    committed_sequence: u64,
    chain: [u8; 32],
    committed_chain: [u8; 32],
    checkpoint_offset: u64,
    checkpoint_len: u64,
    pending_kind: LegacyFrameKind,
    pending_len: u64,
    pending_digest: [u8; 32],
    ack: [u8; 32],
}
impl LegacyStreamHeader {
    pub(crate) fn cursor(&self) -> &[u8; CURSOR_BYTES] {
        &self.cursor
    }
    pub(crate) fn source_version(&self) -> Option<LegacyClosedVersion> {
        match self.source {
            StreamSource::Closed(s) => Some(s),
            StreamSource::Active(_) => None,
        }
    }
    pub(crate) fn context_digest(&self) -> [u8; 32] {
        self.context
    }
    pub(crate) fn committed_end(&self) -> u64 {
        self.committed_end
    }
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }
    pub(crate) fn chain_digest(&self) -> [u8; 32] {
        self.chain
    }
    pub(crate) fn kind(&self) -> LegacyStreamKind {
        self.kind
    }
    pub(crate) fn awaits_checkpoint(&self) -> bool {
        self.phase == Phase::AwaitCheckpoint
    }
    pub(crate) fn done(&self) -> bool {
        self.phase == Phase::Done
    }
    pub(crate) fn resetting(&self) -> bool {
        self.phase == Phase::ResetIntent
    }
    pub(crate) fn pending_kind(&self) -> Option<LegacyFrameKind> {
        if self.pending() {
            Some(self.pending_kind)
        } else {
            None
        }
    }
    pub(crate) fn pending(&self) -> bool {
        self.phase == Phase::AppendIntent
    }
    pub(crate) fn encode(&self, out: &mut [u8; HEADER_BYTES]) {
        out.fill(0);
        out[..8].copy_from_slice(b"CGLEG001");
        out[8] = match self.kind {
            LegacyStreamKind::Association => 1,
            LegacyStreamKind::Staging => 2,
            LegacyStreamKind::Backup => 3,
        };
        out[9] = match self.phase {
            Phase::Running => 1,
            Phase::AwaitCheckpoint => 2,
            Phase::AppendIntent => 3,
            Phase::Done => 4,
            Phase::ResetIntent => 5,
        };
        out[16..1121].copy_from_slice(&self.cursor);
        self.source
            .encode((&mut out[1128..1640]).try_into().unwrap());
        out[1640..1672].copy_from_slice(&self.context);
        for (offset, value) in [
            (1672, self.physical_end),
            (1680, self.committed_end),
            (1688, self.sequence),
            (1696, self.committed_sequence),
            (1768, self.checkpoint_offset),
            (1776, self.checkpoint_len),
            (1792, self.pending_len),
        ] {
            put(&mut out[offset..], value);
        }
        out[1704..1736].copy_from_slice(&self.chain);
        out[1736..1768].copy_from_slice(&self.committed_chain);
        out[1784] = self.pending_kind.byte();
        out[1800..1832].copy_from_slice(&self.pending_digest);
        out[1832..1864].copy_from_slice(&self.ack);
        finish(out);
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, FixedControlError> {
        if bytes.len() != HEADER_BYTES
            || &bytes[..8] != b"CGLEG001"
            || !checked(bytes)
            || bytes[10..16]
                .iter()
                .chain(bytes[1121..1128].iter())
                .chain(bytes[1785..1792].iter())
                .chain(bytes[1864..2016].iter())
                .any(|b| *b != 0)
        {
            return Err(invalid());
        }
        let h = Self {
            kind: match bytes[8] {
                1 => LegacyStreamKind::Association,
                2 => LegacyStreamKind::Staging,
                3 => LegacyStreamKind::Backup,
                _ => return Err(invalid()),
            },
            phase: match bytes[9] {
                1 => Phase::Running,
                2 => Phase::AwaitCheckpoint,
                3 => Phase::AppendIntent,
                4 => Phase::Done,
                5 => Phase::ResetIntent,
                _ => return Err(invalid()),
            },
            cursor: bytes[16..1121].try_into().unwrap(),
            source: StreamSource::decode(bytes[1128..1640].try_into().unwrap())?,
            context: bytes[1640..1672].try_into().unwrap(),
            physical_end: num(&bytes[1672..]),
            committed_end: num(&bytes[1680..]),
            sequence: num(&bytes[1688..]),
            committed_sequence: num(&bytes[1696..]),
            chain: bytes[1704..1736].try_into().unwrap(),
            committed_chain: bytes[1736..1768].try_into().unwrap(),
            checkpoint_offset: num(&bytes[1768..]),
            checkpoint_len: num(&bytes[1776..]),
            pending_kind: LegacyFrameKind::parse(bytes[1784])?,
            pending_len: num(&bytes[1792..]),
            pending_digest: bytes[1800..1832].try_into().unwrap(),
            ack: bytes[1832..1864].try_into().unwrap(),
        };
        if h.committed_end > h.physical_end
            || h.committed_sequence > h.sequence
            || h.pending_len > FRAME_LIMIT as u64
            // Native custody seals retain the complete original header plus
            // retiring source; their authenticated checkpoint is 3968 bytes.
            || (h.checkpoint_len > 2048
                && !(h.pending_kind == LegacyFrameKind::NativeCohort && h.checkpoint_len == 3968))
            || h.checkpoint_offset > h.committed_end
            || (h.phase != Phase::AppendIntent
                && (h.pending_len != 0 || h.pending_digest != [0; 32]))
        {
            return Err(invalid());
        }
        Ok(h)
    }
}
pub(crate) struct LegacyStreamFrame {
    bytes: ReservedBytes,
    kind: LegacyFrameKind,
    sequence: u64,
    context: [u8; 32],
    previous: [u8; 32],
    digest: [u8; 32],
}
impl LegacyStreamFrame {
    pub(crate) fn new(
        kind: LegacyFrameKind,
        context: [u8; 32],
        sequence: u64,
        previous: [u8; 32],
        payload: &[u8],
        memory: &CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let _floor = memory.reserve(8192).map_err(FixedControlError::Resource)?;
        let len = payload
            .len()
            .checked_add(FRAME_PREFIX + FRAME_FOOTER)
            .ok_or_else(invalid)?;
        if len > FRAME_LIMIT {
            return Err(invalid());
        }
        kind.validate(payload)?;
        let mut bytes = memory
            .reserve(len + std::mem::size_of::<Self>())
            .map_err(FixedControlError::Resource)?
            .bytes(len)
            .map_err(FixedControlError::Resource)?;
        let wire = bytes.as_mut_slice();
        wire.fill(0);
        wire[..8].copy_from_slice(b"CGLGFR01");
        wire[8] = kind.byte();
        wire[16..48].copy_from_slice(&context);
        put(&mut wire[48..], sequence);
        put(&mut wire[56..], payload.len() as u64);
        wire[64..96].copy_from_slice(&previous);
        wire[96..96 + payload.len()].copy_from_slice(payload);
        finish(wire);
        let digest = wire[len - 32..].try_into().unwrap();
        Ok(Self {
            bytes,
            kind,
            sequence,
            context,
            previous,
            digest,
        })
    }
    fn read(
        metadata: &PreparedPrivateMetadata,
        offset: u64,
        end: u64,
        memory: &CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let _floor = memory.reserve(256).map_err(FixedControlError::Resource)?;
        let mut prefix = [0; FRAME_PREFIX];
        metadata.read_stream_exact(offset, &mut prefix)?;
        if &prefix[..8] != b"CGLGFR01" || prefix[9..16].iter().any(|b| *b != 0) {
            return Err(invalid());
        }
        let payload = usize::try_from(num(&prefix[56..])).map_err(|_| invalid())?;
        let len = payload
            .checked_add(FRAME_PREFIX + FRAME_FOOTER)
            .ok_or_else(invalid)?;
        if len > FRAME_LIMIT || offset.checked_add(len as u64).is_none_or(|n| n > end) {
            return Err(invalid());
        }
        let mut bytes = memory
            .reserve(len + std::mem::size_of::<Self>())
            .map_err(FixedControlError::Resource)?
            .bytes(len)
            .map_err(FixedControlError::Resource)?;
        metadata.read_stream_exact(offset, bytes.as_mut_slice())?;
        let wire = bytes.as_slice();
        if !checked(wire) || wire[..96] != prefix {
            return Err(invalid());
        }
        let kind = LegacyFrameKind::parse(wire[8])?;
        kind.validate(&wire[96..len - 32])?;
        let result = Self {
            kind,
            sequence: num(&wire[48..]),
            context: wire[16..48].try_into().unwrap(),
            previous: wire[64..96].try_into().unwrap(),
            digest: wire[len - 32..].try_into().unwrap(),
            bytes,
        };
        Ok(result)
    }
    fn payload(&self) -> &[u8] {
        &self.bytes.as_slice()[96..self.bytes.as_slice().len() - 32]
    }
}
pub(crate) enum LegacyStreamAction {
    Start {
        cursor: [u8; CURSOR_BYTES],
        source: LegacyClosedVersion,
        context: [u8; 32],
        kind: LegacyStreamKind,
    },
    StartActive {
        cursor: [u8; CURSOR_BYTES],
        source: ActiveSourceSnapshot,
        context: [u8; 32],
    },
    BeginAppend(LegacyStreamFrame),
    FinishAppend(LegacyStreamFrame),
    Done,
    BeginReset,
    FinishReset,
}
pub(crate) enum CommittedLegacyStreamStep {
    Intent(LegacyStreamHeader),
    Data(LegacyStreamHeader),
    Checkpoint {
        header: LegacyStreamHeader,
        ack: [u8; 32],
    },
    Done(LegacyStreamHeader),
    Reset,
}
pub(crate) struct PreparedLegacyStreamStep {
    metadata: super::maintenance_budget::MemoryBox<PreparedPrivateMetadata>,
    control: super::maintenance_budget::MemoryBox<PreparedControlUpdate>,
    append: Option<LegacyStreamFrame>,
    offset: u64,
    reset: bool,
    next: Option<LegacyStreamHeader>,
    outcome: u8,
    _floor: MemoryReservation,
}
fn start_cursor_matches(
    payload: Option<&[u8]>,
    scope: &RootOriginBinding,
    cursor: &[u8; CURSOR_BYTES],
) -> bool {
    let Some(payload) = payload else {
        return false;
    };
    super::maintenance_candidate::idle_control_cursor(payload, *scope)
        .ok()
        .flatten()
        .is_some_and(|observed| observed.encode() == *cursor)
}
#[inline(never)]
pub(crate) fn prepare_legacy_stream_step(
    target: &MaintenanceTarget,
    scope: &RootOriginBinding,
    action: LegacyStreamAction,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyStreamStep>, FixedControlError> {
    let metadata = boxed_source_metadata(target, scope, memory)?;
    prepare_stream_step_from_metadata(metadata, scope, action, memory)
}

// Metadata preparation completes before the header/action construction frame
// exists. The authenticated metadata stays charged and keeps the same handles.
#[inline(never)]
fn prepare_stream_step_from_metadata(
    metadata: super::maintenance_budget::MemoryBox<PreparedPrivateMetadata>,
    scope: &RootOriginBinding,
    action: LegacyStreamAction,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyStreamStep>, FixedControlError> {
    let floor = memory
        .reserve(std::mem::size_of::<PreparedLegacyStreamStep>() + HEADER_BYTES * 3 + 8192)
        .map_err(FixedControlError::Resource)?;
    let before = metadata.committed().ok_or_else(invalid)?;
    if !matches!(
        metadata.recovery(),
        super::maintenance_controls::RecoveryStep::Committed(_)
    ) {
        return Err(invalid());
    }
    let mut append = None;
    let mut offset = 0;
    let mut reset = false;
    let mut outcome = 0;
    let mut h = match &action {
        LegacyStreamAction::Start {
            cursor,
            source,
            context,
            kind,
        } => {
            if metadata.stream_len() != 0
                || !start_cursor_matches(metadata.payload(), scope, cursor)
            {
                return Err(invalid());
            }
            LegacyStreamHeader {
                cursor: *cursor,
                source: StreamSource::Closed(*source),
                context: *context,
                kind: *kind,
                phase: Phase::Running,
                physical_end: 0,
                committed_end: 0,
                sequence: 0,
                committed_sequence: 0,
                chain: [0; 32],
                committed_chain: [0; 32],
                checkpoint_offset: 0,
                checkpoint_len: 0,
                pending_kind: LegacyFrameKind::CacheCertificate,
                pending_len: 0,
                pending_digest: [0; 32],
                ack: [0; 32],
            }
        }
        LegacyStreamAction::StartActive {
            cursor,
            source,
            context,
        } => {
            if metadata.stream_len() != 0
                || !start_cursor_matches(metadata.payload(), scope, cursor)
            {
                return Err(invalid());
            }
            LegacyStreamHeader {
                cursor: *cursor,
                source: StreamSource::Active(*source),
                context: *context,
                kind: LegacyStreamKind::Backup,
                phase: Phase::Running,
                physical_end: 0,
                committed_end: 0,
                sequence: 0,
                committed_sequence: 0,
                chain: [0; 32],
                committed_chain: [0; 32],
                checkpoint_offset: 0,
                checkpoint_len: 0,
                pending_kind: LegacyFrameKind::CacheCertificate,
                pending_len: 0,
                pending_digest: [0; 32],
                ack: [0; 32],
            }
        }
        _ => LegacyStreamHeader::decode(metadata.payload().ok_or_else(invalid)?)?,
    };
    match action {
        LegacyStreamAction::Start { .. } | LegacyStreamAction::StartActive { .. } => {}
        LegacyStreamAction::BeginAppend(frame) => {
            if !(matches!(h.phase, Phase::Running | Phase::AwaitCheckpoint)
                || h.phase == Phase::Done && frame.kind == LegacyFrameKind::NativeCohort)
                || frame.sequence != h.sequence
                || frame.context != h.context
                || frame.previous != h.chain
            {
                return Err(invalid());
            }
            if frame.kind == LegacyFrameKind::AssociationCheckpoint {
                let checkpoint =
                    LegacyCheckpoint::decode(frame.payload().try_into().map_err(|_| invalid())?)
                        .map_err(|_| invalid())?;
                if checkpoint.context_digest() != h.context || !h.source.matches(&checkpoint) {
                    return Err(invalid());
                }
            }
            if frame.kind == LegacyFrameKind::StagingSeal {
                let seal = super::maintenance_legacy_staging::StagingAssociation::decode(
                    frame.payload().try_into().map_err(|_| invalid())?,
                )
                .map_err(|_| invalid())?;
                if seal.source_context() != h.source.digest() {
                    return Err(invalid());
                }
            }
            if frame.kind == LegacyFrameKind::StagingCheckpoint {
                let checkpoint = StagingProjectionCheckpoint::decode(
                    frame.payload().try_into().map_err(|_| invalid())?,
                )
                .map_err(|_| invalid())?;
                if checkpoint.source_context() != h.source.digest() {
                    return Err(invalid());
                }
            }
            let association = matches!(
                frame.kind,
                LegacyFrameKind::CacheCertificate
                    | LegacyFrameKind::AssociationCheckpoint
                    | LegacyFrameKind::NativeCohort
            );
            if association != (h.kind != LegacyStreamKind::Staging) {
                return Err(invalid());
            }
            if h.kind != LegacyStreamKind::Staging
                && frame.kind != LegacyFrameKind::NativeCohort
                && ((h.phase == Phase::Running && frame.kind != LegacyFrameKind::CacheCertificate)
                    || (h.phase == Phase::AwaitCheckpoint
                        && frame.kind != LegacyFrameKind::AssociationCheckpoint))
            {
                return Err(invalid());
            }
            h.phase = Phase::AppendIntent;
            h.pending_kind = frame.kind;
            h.pending_len = frame.bytes.as_slice().len() as u64;
            h.pending_digest = frame.digest;
            outcome = 1;
        }
        LegacyStreamAction::FinishAppend(frame) => {
            if h.phase != Phase::AppendIntent
                || frame.sequence != h.sequence
                || frame.context != h.context
                || frame.previous != h.chain
                || frame.kind != h.pending_kind
                || frame.digest != h.pending_digest
                || frame.bytes.as_slice().len() as u64 != h.pending_len
            {
                return Err(invalid());
            }
            offset = h.physical_end;
            let end = offset.checked_add(h.pending_len).ok_or_else(invalid)?;
            if metadata.stream_len() < offset || metadata.stream_len() > end {
                return Err(invalid());
            }
            h.physical_end = end;
            h.sequence = h.sequence.checked_add(1).ok_or_else(invalid)?;
            h.chain = frame.digest;
            h.pending_len = 0;
            h.pending_digest = [0; 32];
            if frame.kind.checkpoint() {
                h.phase = Phase::Running;
                h.committed_end = end;
                h.committed_chain = h.chain;
                h.committed_sequence = h.sequence;
                h.checkpoint_offset = offset;
                h.checkpoint_len = frame.payload().len() as u64;
                outcome = 3;
                if frame.kind == LegacyFrameKind::StagingCheckpoint {
                    h.ack = StagingProjectionCheckpoint::decode(
                        frame.payload().try_into().map_err(|_| invalid())?,
                    )
                    .map_err(|_| invalid())?
                    .digest();
                }
            } else {
                h.phase = Phase::AwaitCheckpoint;
                outcome = 2;
                if frame.kind == LegacyFrameKind::CacheCertificate {
                    h.ack = LegacyCacheCertificate::decode(
                        frame.payload().try_into().map_err(|_| invalid())?,
                    )
                    .map_err(|_| invalid())?
                    .digest()
                    .map_err(|_| invalid())?;
                }
            }
            append = Some(frame);
        }
        LegacyStreamAction::Done => {
            if h.phase != Phase::Running || h.physical_end != h.committed_end {
                return Err(invalid());
            }
            h.phase = Phase::Done;
            outcome = 4;
        }
        LegacyStreamAction::BeginReset => {
            if h.phase != Phase::Done {
                return Err(invalid());
            }
            validate_prefix(&metadata, &h, memory)?;
            h.phase = Phase::ResetIntent;
            outcome = 1;
        }
        LegacyStreamAction::FinishReset => {
            if h.phase != Phase::ResetIntent {
                return Err(invalid());
            }
            if metadata.stream_len() == h.physical_end {
                validate_prefix(&metadata, &h, memory)?;
                reset = true;
                offset = h.physical_end;
            } else if metadata.stream_len() != 0 {
                return Err(invalid());
            }
            outcome = 5;
        }
    }
    let mut image = [0; HEADER_BYTES];
    h.encode(&mut image);
    let payload = if outcome == 5 {
        &h.cursor[..]
    } else {
        &image[..]
    };
    let control = boxed_stream_control(scope, before, payload, memory)?;
    super::maintenance_budget::MemoryBox::new(
        PreparedLegacyStreamStep {
            metadata,
            control,
            append,
            offset,
            reset,
            next: if outcome == 5 { None } else { Some(h) },
            outcome,
            _floor: floor,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_stream_control(
    scope: &RootOriginBinding,
    before: super::maintenance_controls::CommitFact,
    payload: &[u8],
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedControlUpdate>, FixedControlError> {
    let control = admission::prepare_metadata_control(scope, before, payload, memory)?;
    super::maintenance_budget::MemoryBox::new(control, memory).map_err(FixedControlError::Resource)
}

impl PreparedLegacyStreamStep {
    pub(crate) fn effects(&self) -> usize {
        self.control.effects()
            + if self.append.is_some() {
                3
            } else if self.reset {
                2
            } else {
                0
            }
    }
    pub(crate) fn advance(
        &mut self,
        grant: &mut MutationGrant,
    ) -> Result<CommittedLegacyStreamStep, FixedControlError> {
        self.advance_inner(None, grant)
    }
    /// The outer genuine source role must reserve effects() before acquiring
    /// its guards. This borrows and verifies its actual exclusive root lease.
    pub(crate) fn advance_with_owned_root(
        &mut self,
        root_use: &super::locks::FixedLockedFile,
        grant: &mut MutationGrant,
    ) -> Result<CommittedLegacyStreamStep, FixedControlError> {
        self.advance_inner(Some(root_use), grant)
    }
    fn advance_inner(
        &mut self,
        root_use: Option<&super::locks::FixedLockedFile>,
        grant: &mut MutationGrant,
    ) -> Result<CommittedLegacyStreamStep, FixedControlError> {
        if self.outcome == 255 {
            return Err(invalid());
        }
        let effects = self.effects();
        let append = &self.append;
        let control = &self.control;
        let offset = self.offset;
        let reset = self.reset;
        let mut apply = |mutation: &mut admission::PrivateMetadataMutation<'_>,
                         grant: &mut MutationGrant| {
            if let Some(frame) = append {
                mutation.append_frame(offset, frame.bytes.as_slice(), grant)?;
            }
            if reset {
                mutation.reset_stream(offset, grant)?;
            }
            mutation.publish_control(control, grant)
        };
        if let Some(root_use) = root_use {
            admission::with_reserved_private_metadata_owned_root(
                &mut self.metadata,
                root_use,
                grant,
                effects,
                &mut apply,
            )?;
        } else {
            admission::with_private_metadata(&mut self.metadata, grant, effects, &mut apply)?;
        }
        let outcome = self.outcome;
        self.outcome = 255;
        if outcome == 5 {
            return Ok(CommittedLegacyStreamStep::Reset);
        }
        let header = self.next.unwrap();
        Ok(match outcome {
            1 => CommittedLegacyStreamStep::Intent(header),
            2 => CommittedLegacyStreamStep::Data(header),
            3 => CommittedLegacyStreamStep::Checkpoint {
                ack: header.ack,
                header,
            },
            4 => CommittedLegacyStreamStep::Done(header),
            _ => CommittedLegacyStreamStep::Data(header),
        })
    }
}
fn scan(
    metadata: &PreparedPrivateMetadata,
    h: &LegacyStreamHeader,
    memory: &CleanupMemory,
    end: u64,
    emit: &mut dyn FnMut(u64, &LegacyStreamFrame) -> Result<(), FixedControlError>,
) -> Result<(), FixedControlError> {
    let mut offset = 0u64;
    let mut sequence = 0u64;
    let mut previous = [0; 32];
    while offset < end {
        let frame = LegacyStreamFrame::read(metadata, offset, end, memory)?;
        if frame.sequence != sequence || frame.previous != previous || frame.context != h.context {
            return Err(invalid());
        }
        emit(offset, &frame)?;
        previous = frame.digest;
        sequence = sequence.checked_add(1).ok_or_else(invalid)?;
        offset = offset
            .checked_add(frame.bytes.as_slice().len() as u64)
            .ok_or_else(invalid)?;
    }
    if offset != end || sequence != h.committed_sequence || previous != h.committed_chain {
        return Err(invalid());
    }
    Ok(())
}
fn validate_prefix(
    metadata: &PreparedPrivateMetadata,
    h: &LegacyStreamHeader,
    memory: &CleanupMemory,
) -> Result<(), FixedControlError> {
    if h.committed_end != h.physical_end {
        return Err(invalid());
    }
    scan(metadata, h, memory, h.committed_end, &mut |_, _| Ok(()))
}

pub(crate) struct LegacyStreamReplay<'a> {
    metadata: &'a PreparedPrivateMetadata,
    header: LegacyStreamHeader,
    memory: &'a CleanupMemory,
    _floor: MemoryReservation,
}
impl<'a> LegacyStreamReplay<'a> {
    pub(crate) fn new(
        metadata: &'a PreparedPrivateMetadata,
        expected: &LegacyClosedVersion,
        context: [u8; 32],
        memory: &'a CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let floor = memory
            .reserve(std::mem::size_of::<Self>() + 8192)
            .map_err(FixedControlError::Resource)?;
        let header = LegacyStreamHeader::decode(metadata.payload().ok_or_else(invalid)?)?;
        if header.source.digest() != expected.context_digest()
            || header.context != context
            || metadata.stream_len() < header.committed_end
        {
            return Err(invalid());
        }
        Ok(Self {
            metadata,
            header,
            memory,
            _floor: floor,
        })
    }
    pub(crate) fn new_active(
        metadata: &'a PreparedPrivateMetadata,
        expected: &ActiveSourceSnapshot,
        context: [u8; 32],
        memory: &'a CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let floor = memory
            .reserve(std::mem::size_of::<Self>() + 8192)
            .map_err(FixedControlError::Resource)?;
        let header = LegacyStreamHeader::decode(metadata.payload().ok_or_else(invalid)?)?;
        if header.kind != LegacyStreamKind::Backup
            || header.source.digest() != expected.context_digest()
            || header.context != context
            || metadata.stream_len() < header.committed_end
        {
            return Err(invalid());
        }
        Ok(Self {
            metadata,
            header,
            memory,
            _floor: floor,
        })
    }
    pub(crate) fn checkpoint(&self) -> Result<ReservedBytes, FixedControlError> {
        if self.header.checkpoint_len == 0 {
            return Err(invalid());
        }
        let frame = LegacyStreamFrame::read(
            self.metadata,
            self.header.checkpoint_offset,
            self.header.committed_end,
            self.memory,
        )?;
        if !frame.kind.checkpoint()
            || frame.context != self.header.context
            || frame.payload().len() as u64 != self.header.checkpoint_len
        {
            return Err(invalid());
        }
        let mut bytes = self
            .memory
            .reserve(frame.payload().len())
            .map_err(FixedControlError::Resource)?
            .bytes(frame.payload().len())
            .map_err(FixedControlError::Resource)?;
        bytes.as_mut_slice().copy_from_slice(frame.payload());
        Ok(bytes)
    }
}
fn legacy_error(e: FixedControlError) -> LegacyIssue {
    match e {
        FixedControlError::Resource(e) => LegacyIssue::Resource(e),
        FixedControlError::Io(e) => LegacyIssue::Io(e.kind()),
        _ => LegacyIssue::Unknown,
    }
}
fn staging_error(e: FixedControlError) -> StagingIssue {
    match e {
        FixedControlError::Resource(e) => StagingIssue::Resource(e),
        FixedControlError::Io(e) => StagingIssue::Io(e.kind()),
        _ => StagingIssue::Unknown,
    }
}
impl ReplayLegacyCertificates for LegacyStreamReplay<'_> {
    fn replay(
        &self,
        memory: &CleanupMemory,
        emit: &mut dyn FnMut(LegacyCacheCertificate) -> Result<(), LegacyIssue>,
    ) -> Result<(), LegacyIssue> {
        if self.header.kind == LegacyStreamKind::Staging {
            return Err(LegacyIssue::Unknown);
        }
        // Preserve the consumer's precise error while the transport callback uses its
        // own fixed control error type. No error string or unbounded result is kept.
        let mut consumer = None;
        let result = scan(
            self.metadata,
            &self.header,
            memory,
            self.header.committed_end,
            &mut |_, frame| {
                match frame.kind {
                    LegacyFrameKind::CacheCertificate => {
                        let cert = LegacyCacheCertificate::decode(
                            frame.payload().try_into().map_err(|_| invalid())?,
                        )
                        .map_err(|_| invalid())?;
                        if let Err(e) = emit(cert) {
                            consumer = Some(e);
                            return Err(invalid());
                        }
                    }
                    LegacyFrameKind::AssociationCheckpoint | LegacyFrameKind::NativeCohort => {}
                    _ => return Err(invalid()),
                }
                Ok(())
            },
        );
        if let Some(error) = consumer {
            return Err(error);
        }
        result.map_err(legacy_error)
    }
}
impl ReplayStagingCertificates for LegacyStreamReplay<'_> {
    fn replay(
        &self,
        emit: &mut dyn FnMut(StagingReplayEvent) -> Result<(), StagingIssue>,
    ) -> Result<(), StagingIssue> {
        if self.header.kind != LegacyStreamKind::Staging {
            return Err(StagingIssue::Unknown);
        }
        let mut start = 0u64;
        let mut start_sequence = 0;
        let mut start_previous = [0; 32];
        let mut consumer = None;
        let result = scan(
            self.metadata,
            &self.header,
            self.memory,
            self.header.committed_end,
            &mut |offset, frame| {
                match frame.kind {
                    LegacyFrameKind::StagingRows
                    | LegacyFrameKind::StagingCheckpoint
                    | LegacyFrameKind::StagingSeal => {}
                    LegacyFrameKind::StagingPayload => {
                        let cert = StagingPayloadCertificate::decode(
                            frame.payload().try_into().map_err(|_| invalid())?,
                        )
                        .map_err(|_| invalid())?;
                        if cert.source_context != self.header.source.digest() {
                            return Err(invalid());
                        }
                        if let Err(e) = emit(StagingReplayEvent::Payload(cert)) {
                            consumer = Some(e);
                            return Err(invalid());
                        }
                        let mut at = start;
                        let mut sequence = start_sequence;
                        let mut previous = start_previous;
                        while at < offset {
                            let item =
                                LegacyStreamFrame::read(self.metadata, at, offset, self.memory)?;
                            if item.context != self.header.context
                                || item.sequence != sequence
                                || item.previous != previous
                            {
                                return Err(invalid());
                            }
                            match item.kind {
                                LegacyFrameKind::StagingRows => {
                                    for row in item.payload().chunks_exact(768) {
                                        let row =
                                            StagingRowWitness::decode(row.try_into().unwrap())
                                                .map_err(|_| invalid())?;
                                        if let Err(e) = emit(StagingReplayEvent::Row(row)) {
                                            consumer = Some(e);
                                            return Err(invalid());
                                        }
                                    }
                                }
                                LegacyFrameKind::StagingCheckpoint => {}
                                _ => return Err(invalid()),
                            }
                            at = at
                                .checked_add(item.bytes.as_slice().len() as u64)
                                .ok_or_else(invalid)?;
                            sequence = sequence.checked_add(1).ok_or_else(invalid)?;
                            previous = item.digest;
                        }
                        if at != offset || sequence != frame.sequence || previous != frame.previous
                        {
                            return Err(invalid());
                        }
                        if let Err(e) = emit(StagingReplayEvent::EndPayload) {
                            consumer = Some(e);
                            return Err(invalid());
                        }
                        start = offset
                            .checked_add(frame.bytes.as_slice().len() as u64)
                            .ok_or_else(invalid)?;
                        start_sequence = frame.sequence.checked_add(1).ok_or_else(invalid)?;
                        start_previous = frame.digest;
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
        );
        if let Some(error) = consumer {
            return Err(error);
        }
        result.map_err(staging_error)
    }
}

// Complete transfers the existing real source pair off authority; bounded preparation stages account for this fixed transport layout.
#[allow(clippy::large_enum_variant)]
pub(crate) enum AssociationSourceProgress {
    Pending,
    Step(super::maintenance_budget::MemoryBox<PreparedLegacyStreamStep>),
    Complete {
        association: super::maintenance_legacy_association::LegacyAssociation,
        database: std::fs::File,
        manifest: std::fs::File,
    },
}
pub(super) struct AssociationTransportState {
    source: StreamSource,
    context: [u8; 32],
    pending: Option<LegacyCacheCertificate>,
    checkpoint: Option<LegacyCheckpoint>,
    completed: Option<super::maintenance_budget::MemoryBox<AssociationComplete>>,
    reset_complete: bool,
    _floor: MemoryReservation,
}
struct AssociationComplete {
    association: super::maintenance_legacy_association::LegacyAssociation,
    database: std::fs::File,
    manifest: std::fs::File,
    _floor: MemoryReservation,
}
fn source_equal(a: StreamSource, b: StreamSource) -> bool {
    a.digest() == b.digest()
}
fn make_frame(
    kind: LegacyFrameKind,
    header: &LegacyStreamHeader,
    transport: &AssociationTransportState,
    memory: &CleanupMemory,
) -> Result<LegacyStreamFrame, FixedControlError> {
    let _floor = memory.reserve(8192).map_err(FixedControlError::Resource)?;
    match kind {
        LegacyFrameKind::CacheCertificate => {
            let mut bytes = [0; 768];
            transport.pending.ok_or_else(invalid)?.encode(&mut bytes);
            LegacyStreamFrame::new(
                kind,
                header.context,
                header.sequence,
                header.chain,
                &bytes,
                memory,
            )
        }
        LegacyFrameKind::AssociationCheckpoint => {
            let mut bytes = [0; 2048];
            transport
                .checkpoint
                .as_ref()
                .ok_or_else(invalid)?
                .encode(&mut bytes);
            LegacyStreamFrame::new(
                kind,
                header.context,
                header.sequence,
                header.chain,
                &bytes,
                memory,
            )
        }
        _ => Err(invalid()),
    }
}
/// One advisory advance, or one prebound short stream mutation. The state is
/// parked on the joined preparation worker and never dropped under admission.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_association_source(
    target: &MaintenanceTarget,
    domain: &super::maintenance_ownership::ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    database: std::fs::File,
    manifest: std::fs::File,
    artifact_root: super::locks::RootedDirectory,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    owner: Option<&super::maintenance_legacy_bootstrap::LegacySourceOwner>,
    backup: Option<super::maintenance_backup::BackupAddress>,
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<AssociationSourceProgress, FixedControlError> {
    if cancel() {
        return Err(invalid());
    }
    let scope = association_origin(target, domain, expected, memory)?;
    prepare_association_source_inner(
        target,
        scope,
        domain,
        expected,
        Some((database, manifest, artifact_root)),
        artifact_root_path,
        repository_root,
        owner,
        backup,
        state,
        memory,
        cancel,
    )
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_parked_association_source(
    target: &MaintenanceTarget,
    domain: &super::maintenance_ownership::ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    owner: Option<&super::maintenance_legacy_bootstrap::LegacySourceOwner>,
    backup: Option<super::maintenance_backup::BackupAddress>,
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<AssociationSourceProgress, FixedControlError> {
    if cancel() {
        return Err(invalid());
    }
    let scope = association_origin(target, domain, expected, memory)?;
    prepare_association_source_inner(
        target,
        scope,
        domain,
        expected,
        None,
        artifact_root_path,
        repository_root,
        owner,
        backup,
        state,
        memory,
        cancel,
    )
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn prepare_association_source_inner(
    target: &MaintenanceTarget,
    scope: RootOriginBinding,
    domain: &super::maintenance_ownership::ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    mut inputs: Option<(std::fs::File, std::fs::File, super::locks::RootedDirectory)>,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    owner: Option<&super::maintenance_legacy_bootstrap::LegacySourceOwner>,
    backup: Option<super::maintenance_backup::BackupAddress>,
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<AssociationSourceProgress, FixedControlError> {
    let _floor = memory.reserve(65536).map_err(FixedControlError::Resource)?;
    if cancel() {
        return Err(invalid());
    }
    let mut source = observe_association_source(
        domain,
        expected,
        inputs.as_ref(),
        artifact_root_path,
        repository_root,
        owner,
        backup.is_some(),
        state,
        memory,
        cancel,
    )?;
    let metadata = boxed_source_metadata(target, &scope, memory)?;
    let observed = boxed_observed_header(&metadata, memory)?;
    if let Some(header) = observed.as_ref() {
        if let (StreamSource::Closed(original), StreamSource::Closed(actual)) =
            (header.source, source)
        {
            if !actual.same_job_source(&original) {
                return Err(invalid());
            }
            source = StreamSource::Closed(original);
        }
        if !source_equal(header.source, source)
            || (backup.is_some() != (header.kind == LegacyStreamKind::Backup))
        {
            return Err(invalid());
        }
    } else if super::maintenance_candidate::idle_control_cursor(
        metadata.payload().ok_or_else(invalid)?,
        scope,
    )
    .map_err(|_| invalid())?
    .is_none()
    {
        return Err(invalid());
    }
    if let Some(transport) = state.transport.as_mut() {
        if !source_equal(transport.source, source) {
            return Err(invalid());
        }
        if transport.reset_complete {
            return take_association_completion(state);
        }
    }
    if state.job.is_none()
        && state
            .transport
            .as_ref()
            .is_none_or(|t| t.completed.is_none())
    {
        let checkpoint = if let Some(header) = observed.as_ref() {
            let replay = match source {
                StreamSource::Closed(original) => {
                    LegacyStreamReplay::new(&metadata, &original, header.context, memory)?
                }
                StreamSource::Active(original) => {
                    LegacyStreamReplay::new_active(&metadata, &original, header.context, memory)?
                }
            };
            replay.checkpoint().ok()
        } else {
            None
        };
        let (database, manifest, artifact_root) = inputs.take().ok_or_else(invalid)?;
        let job = boxed_source_job(
            database,
            manifest,
            artifact_root,
            artifact_root_path,
            repository_root,
            expected,
            backup,
            checkpoint.as_ref().map(|bytes| bytes.as_slice()),
            memory,
        )?;
        let context = job
            .checkpoint()
            .map_err(association_error)?
            .context_digest();
        if observed.as_ref().is_some_and(|h| h.context != context) {
            return Err(invalid());
        }
        state.job = Some(job);
        if state.transport.is_none() {
            state.transport = Some(AssociationTransportState {
                source,
                context,
                pending: None,
                checkpoint: None,
                completed: None,
                reset_complete: false,
                _floor: memory
                    .reserve(std::mem::size_of::<AssociationTransportState>() + 8192)
                    .map_err(FixedControlError::Resource)?,
            });
        }
    } else {
        if let Some((database, manifest, artifact_root)) = inputs.take() {
            if let Some(job) = state.job.as_ref() {
                if !job.source_matches(
                    &database,
                    &manifest,
                    &artifact_root,
                    artifact_root_path,
                    repository_root,
                    expected,
                ) {
                    return Err(invalid());
                }
            }
            drop((database, manifest, artifact_root));
        }
    }
    if let (Some(job), Some(owner)) = (state.job.as_mut(), owner) {
        job.authorize_closed_source(owner, memory)
            .map_err(association_error)?;
    }
    if state
        .transport
        .as_ref()
        .is_some_and(|transport| transport.completed.is_some())
        && (matches!(source, StreamSource::Closed(_)) || backup.is_some())
        && observed.as_ref().is_some_and(|header| header.done())
    {
        return take_association_completion(state);
    }
    if let Some(action) = association_stream_action(
        &metadata,
        &scope,
        source,
        observed.as_deref(),
        state.transport.as_ref().ok_or_else(invalid)?,
        memory,
    )? {
        drop(metadata);
        return Ok(AssociationSourceProgress::Step(boxed_association_step(
            target,
            &scope,
            action.into_inner(),
            memory,
        )?));
    }
    advance_association_producer(domain, owner, state, &metadata, source, memory, cancel)?;
    // A preparation callback may be the last one before worker shutdown. Carry
    // its new certificate or seal into the durable stream in this same callback
    // rather than requiring a warm-only Pending callback to survive a restart.
    if (matches!(source, StreamSource::Closed(_)) || backup.is_some())
        && observed.as_ref().is_some_and(|header| header.done())
        && state
            .transport
            .as_ref()
            .is_some_and(|transport| transport.completed.is_some())
    {
        return take_association_completion(state);
    }
    let action = association_stream_action(
        &metadata,
        &scope,
        source,
        observed.as_deref(),
        state.transport.as_ref().ok_or_else(invalid)?,
        memory,
    )?;
    if let Some(action) = action {
        drop(metadata);
        return Ok(AssociationSourceProgress::Step(boxed_association_step(
            target,
            &scope,
            action.into_inner(),
            memory,
        )?));
    }
    Ok(AssociationSourceProgress::Pending)
}

// Decide and retain only the next short mutation, outside the large source
// preparation frame. No extra stream or payload descriptors are opened here.
#[inline(never)]
fn association_stream_action(
    metadata: &PreparedPrivateMetadata,
    scope: &RootOriginBinding,
    source: StreamSource,
    header: Option<&LegacyStreamHeader>,
    transport: &AssociationTransportState,
    memory: &CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<LegacyStreamAction>>, FixedControlError> {
    if header.is_none() {
        if metadata.stream_len() != 0 {
            return Err(invalid());
        }
        let cursor = super::maintenance_candidate::idle_control_cursor(
            metadata.payload().ok_or_else(invalid)?,
            *scope,
        )
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?
        .encode();
        let action = match source {
            StreamSource::Closed(source) => LegacyStreamAction::Start {
                cursor,
                source,
                context: transport.context,
                kind: LegacyStreamKind::Association,
            },
            StreamSource::Active(source) => LegacyStreamAction::StartActive {
                cursor,
                source,
                context: transport.context,
            },
        };
        return super::maintenance_budget::MemoryBox::new(action, memory)
            .map(Some)
            .map_err(FixedControlError::Resource);
    }
    let header = header.unwrap();
    if transport.completed.is_some() {
        let action = if header.resetting() {
            LegacyStreamAction::FinishReset
        } else if header.done() {
            LegacyStreamAction::BeginReset
        } else {
            LegacyStreamAction::Done
        };
        return super::maintenance_budget::MemoryBox::new(action, memory)
            .map(Some)
            .map_err(FixedControlError::Resource);
    }
    if transport.pending.is_some() {
        let kind = if let Some(kind) = header.pending_kind() {
            kind
        } else if header.awaits_checkpoint() {
            LegacyFrameKind::AssociationCheckpoint
        } else {
            LegacyFrameKind::CacheCertificate
        };
        let frame = make_frame(kind, header, transport, memory)?;
        let action = if header.pending() {
            LegacyStreamAction::FinishAppend(frame)
        } else {
            LegacyStreamAction::BeginAppend(frame)
        };
        return super::maintenance_budget::MemoryBox::new(action, memory)
            .map(Some)
            .map_err(FixedControlError::Resource);
    }
    Ok(None)
}

#[inline(never)]
fn take_association_completion(
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
) -> Result<AssociationSourceProgress, FixedControlError> {
    let completed = state
        .transport
        .as_mut()
        .ok_or_else(invalid)?
        .completed
        .take()
        .ok_or_else(invalid)?
        .into_inner();
    state.clear();
    Ok(AssociationSourceProgress::Complete {
        association: completed.association,
        database: completed.database,
        manifest: completed.manifest,
    })
}

// Keep native producer results and completion moves out of stream preparation's
// frame. These stages run on the same joined, off-admission worker.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn advance_association_producer(
    domain: &super::maintenance_ownership::ReferenceDomain,
    owner: Option<&super::maintenance_legacy_bootstrap::LegacySourceOwner>,
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    metadata: &PreparedPrivateMetadata,
    source: StreamSource,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<(), FixedControlError> {
    use super::maintenance_budget::MemoryBox;
    use super::maintenance_legacy_association::LegacyProgress;
    let transport = state.transport.as_mut().ok_or_else(invalid)?;
    let replay = match source {
        StreamSource::Closed(original) => {
            LegacyStreamReplay::new(metadata, &original, transport.context, memory)?
        }
        StreamSource::Active(original) => {
            LegacyStreamReplay::new_active(metadata, &original, transport.context, memory)?
        }
    };
    // Catalog/resume/end-of-cursor transitions validate no cache payload.
    // Continue those finite stages until one CacheValidated result, then stop:
    // every preparation still validates at most one actual cache payload.
    loop {
        if cancel() {
            return Err(invalid());
        }
        let progress = state
            .job
            .as_mut()
            .ok_or_else(invalid)?
            .advance_with_certificates(domain, memory, cancel, &replay)
            .map_err(association_error)?;
        if let Some(owner) = owner {
            if !owner.is_current(memory)? {
                return Err(invalid());
            }
        }
        match progress {
            LegacyProgress::Pending => continue,
            LegacyProgress::CacheValidated(cert) => {
                transport.pending = Some(cert);
                transport.checkpoint = Some(
                    state
                        .job
                        .as_ref()
                        .unwrap()
                        .checkpoint_after_cache()
                        .map_err(association_error)?,
                );
                return Ok(());
            }
            LegacyProgress::Complete(association) => {
                let (database, manifest) =
                    state.job.take().unwrap().into_inner().into_source_handles();
                transport.completed = Some(
                    MemoryBox::new(
                        AssociationComplete {
                            association,
                            database,
                            manifest,
                            _floor: memory
                                .reserve(std::mem::size_of::<AssociationComplete>())
                                .map_err(FixedControlError::Resource)?,
                        },
                        memory,
                    )
                    .map_err(FixedControlError::Resource)?,
                );
                return Ok(());
            }
        }
    }
}
fn association_error(error: LegacyIssue) -> FixedControlError {
    match error {
        LegacyIssue::Resource(e) => FixedControlError::Resource(e),
        LegacyIssue::Io(e) => FixedControlError::Io(e.into()),
        _ => invalid(),
    }
}
/// Called only by the driver's offpermit finish path, after the exact prepared
/// mutation returned. This is where warm producer cursors are acknowledged.
pub(crate) fn finish_association_source_step(
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    result: CommittedLegacyStreamStep,
) -> Result<(), FixedControlError> {
    let transport = state.transport.as_mut().ok_or_else(invalid)?;
    match result {
        CommittedLegacyStreamStep::Checkpoint { header, ack } => {
            if header.context != transport.context || !source_equal(header.source, transport.source)
            {
                return Err(invalid());
            }
            let cert = transport.pending.ok_or_else(invalid)?;
            if cert.digest().map_err(association_error)? != ack {
                return Err(invalid());
            }
            state
                .job
                .as_mut()
                .ok_or_else(invalid)?
                .acknowledge_cache(ack)
                .map_err(association_error)?;
            transport.pending = None;
            transport.checkpoint = None;
        }
        CommittedLegacyStreamStep::Reset => {
            transport.reset_complete = true;
        }
        CommittedLegacyStreamStep::Intent(h)
        | CommittedLegacyStreamStep::Data(h)
        | CommittedLegacyStreamStep::Done(h) => {
            if h.context != transport.context || !source_equal(h.source, transport.source) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn observe_association_source(
    domain: &super::maintenance_ownership::ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    inputs: Option<&(std::fs::File, std::fs::File, super::locks::RootedDirectory)>,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    owner: Option<&super::maintenance_legacy_bootstrap::LegacySourceOwner>,
    backup_selected: bool,
    state: &mut super::maintenance_legacy_association::SelectedBackupAssociationState,
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<StreamSource, FixedControlError> {
    let source = if let Some((database, manifest, artifact_root)) = inputs {
        if let Some(owner) = owner {
            if !owner.is_current(memory)? {
                return Err(invalid());
            }
            let original = owner.closed_version();
            if original.database_stamp()
                != Some(super::locks::FixedFileStamp::capture(
                    database,
                    super::locks::FixedControlKind::File,
                )?)
                || original.manifest_stamp()
                    != Some(super::locks::FixedFileStamp::capture(
                        manifest,
                        super::locks::FixedControlKind::File,
                    )?)
                || original.cache_identity() != artifact_root.identity()
            {
                return Err(invalid());
            }
            StreamSource::Closed(original)
        } else {
            if !backup_selected {
                return Err(invalid());
            }
            StreamSource::Active(ActiveSourceSnapshot::observe(
                database,
                manifest,
                artifact_root,
                domain,
                expected,
                memory,
            )?)
        }
    } else {
        let transport = state.transport.as_ref().ok_or_else(invalid)?;
        if matches!(transport.source, StreamSource::Closed(_)) != owner.is_some() {
            return Err(invalid());
        }
        if let Some(owner) = owner {
            if !owner.is_current(memory)?
                || !match transport.source {
                    StreamSource::Closed(original) => {
                        owner.closed_version().same_job_source(&original)
                    }
                    _ => false,
                }
            {
                return Err(invalid());
            }
        }
        if let Some(job) = state.job.as_mut() {
            if let Some(owner) = owner {
                job.authorize_closed_source(owner, memory)
                    .map_err(association_error)?;
            }
            job.check_parked_source(
                domain,
                expected,
                artifact_root_path,
                repository_root,
                memory,
                cancel,
            )
            .map_err(association_error)?;
        } else if let Some(done) = transport.completed.as_ref() {
            done.association.database_stamp().recheck(&done.database)?;
            done.association.manifest_stamp().recheck(&done.manifest)?;
            if !super::maintenance_ownership::expected_reference_current(domain, expected, memory)?
            {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
        transport.source
    };
    Ok(source)
}

#[inline(never)]
fn association_origin(
    target: &MaintenanceTarget,
    domain: &super::maintenance_ownership::ReferenceDomain,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    memory: &CleanupMemory,
) -> Result<RootOriginBinding, FixedControlError> {
    let prepared = admission::prepare_canonical(target, domain, expected, memory)?;
    Ok(prepared.origin_scope())
}
// The authenticated header is retained across source setup and stream steps;
// keep its cursor image charged on the heap instead of copying it per branch.
#[inline(never)]
fn boxed_observed_header(
    metadata: &PreparedPrivateMetadata,
    memory: &CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<LegacyStreamHeader>>, FixedControlError> {
    metadata
        .payload()
        .and_then(|bytes| LegacyStreamHeader::decode(bytes).ok())
        .map(|header| {
            super::maintenance_budget::MemoryBox::new(header, memory)
                .map_err(FixedControlError::Resource)
        })
        .transpose()
}

#[inline(never)]
fn boxed_source_metadata(
    target: &MaintenanceTarget,
    scope: &RootOriginBinding,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedPrivateMetadata>, FixedControlError> {
    let prepared = admission::prepare_private_metadata(target, scope, memory)?;
    super::maintenance_budget::MemoryBox::new(prepared, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_association_step(
    target: &MaintenanceTarget,
    scope: &RootOriginBinding,
    action: LegacyStreamAction,
    memory: &CleanupMemory,
) -> Result<super::maintenance_budget::MemoryBox<PreparedLegacyStreamStep>, FixedControlError> {
    prepare_legacy_stream_step(target, scope, action, memory)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn boxed_source_job(
    database: std::fs::File,
    manifest: std::fs::File,
    artifact_root: super::locks::RootedDirectory,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    backup: Option<super::maintenance_backup::BackupAddress>,
    checkpoint: Option<&[u8]>,
    memory: &CleanupMemory,
) -> Result<
    super::maintenance_budget::MemoryBox<
        super::maintenance_legacy_association::LegacyAssociationPreparation,
    >,
    FixedControlError,
> {
    use super::maintenance_legacy_association::{LegacyAssociationPreparation, LegacyCheckpoint};
    let job = if let Some(bytes) = checkpoint {
        LegacyAssociationPreparation::restore(
            database,
            manifest,
            artifact_root,
            artifact_root_path,
            repository_root,
            LegacyCheckpoint::decode(bytes.try_into().map_err(|_| invalid())?)
                .map_err(association_error)?,
            memory,
        )
        .map_err(association_error)?
    } else {
        return boxed_new_source_job(
            database,
            manifest,
            artifact_root,
            artifact_root_path,
            repository_root,
            expected,
            backup,
            memory,
        );
    };
    super::maintenance_budget::MemoryBox::new(job, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn boxed_new_source_job(
    database: std::fs::File,
    manifest: std::fs::File,
    artifact_root: super::locks::RootedDirectory,
    artifact_root_path: &std::path::Path,
    repository_root: &std::path::Path,
    expected: &super::maintenance_ownership::ReferenceEpoch,
    backup: Option<super::maintenance_backup::BackupAddress>,
    memory: &CleanupMemory,
) -> Result<
    super::maintenance_budget::MemoryBox<
        super::maintenance_legacy_association::LegacyAssociationPreparation,
    >,
    FixedControlError,
> {
    let mut job = super::maintenance_legacy_association::LegacyAssociationPreparation::new(
        database,
        manifest,
        artifact_root,
        artifact_root_path,
        repository_root,
        expected.clone(),
        memory,
    )
    .map_err(association_error)?;
    if let Some(backup) = backup {
        job.select_backup_cache(backup.key(), backup.name())
            .map_err(association_error)?;
    }
    super::maintenance_budget::MemoryBox::new(job, memory).map_err(FixedControlError::Resource)
}

impl AssociationTransportState {
    pub(super) fn has_completed(&self) -> bool {
        self.completed.is_some()
    }
}

impl LegacyStreamReplay<'_> {
    /// Reads only the seal from an already authenticated, complete private
    /// prefix. The caller must additionally verify rooted source custody and
    /// exact certificate membership before constructing an effect capability.
    pub(crate) fn staging_seal(
        &self,
    ) -> Result<Option<super::maintenance_legacy_staging::StagingAssociation>, FixedControlError>
    {
        if self.header.kind != LegacyStreamKind::Staging || self.header.checkpoint_len != 512 {
            return Ok(None);
        }
        scan(
            self.metadata,
            &self.header,
            self.memory,
            self.header.committed_end,
            &mut |_, _| Ok(()),
        )?;
        let frame = LegacyStreamFrame::read(
            self.metadata,
            self.header.checkpoint_offset,
            self.header.committed_end,
            self.memory,
        )?;
        if frame.kind != LegacyFrameKind::StagingSeal || frame.context != self.header.context {
            return Err(invalid());
        }
        let seal = super::maintenance_legacy_staging::StagingAssociation::decode(
            frame.payload().try_into().map_err(|_| invalid())?,
        )
        .map_err(|_| invalid())?;
        if seal.source_context() != self.header.source.digest() {
            return Err(invalid());
        }
        Ok(Some(seal))
    }
}

impl LegacyStreamHeader {
    pub(crate) fn before_native_cohort(&self) -> Result<Self, FixedControlError> {
        if self.kind != LegacyStreamKind::Association {
            return Err(invalid());
        }
        if self.phase == Phase::Done {
            return Ok(*self);
        }
        if self.phase != Phase::AppendIntent || self.pending_kind != LegacyFrameKind::NativeCohort {
            return Err(invalid());
        }
        let mut original = *self;
        original.phase = Phase::Done;
        original.pending_kind = LegacyFrameKind::AssociationCheckpoint;
        original.pending_len = 0;
        original.pending_digest = [0; 32];
        Ok(original)
    }
}
impl<'a> LegacyStreamReplay<'a> {
    pub(crate) fn from_header(
        metadata: &'a PreparedPrivateMetadata,
        original: LegacyStreamHeader,
        memory: &'a CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        if !original.done() || metadata.stream_len() < original.committed_end {
            return Err(invalid());
        }
        validate_prefix(metadata, &original, memory)?;
        Ok(Self {
            metadata,
            header: original,
            memory,
            _floor: memory
                .reserve(std::mem::size_of::<Self>() + 8192)
                .map_err(FixedControlError::Resource)?,
        })
    }
}
