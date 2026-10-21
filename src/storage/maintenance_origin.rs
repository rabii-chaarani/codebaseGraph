//! Actual producer origin for one artifact payload leaf. Origin is independent
//! of loader usability, reachability, migration and deletion authority.
use super::locks::{self, FileIdentity, FixedControlKind, FixedFileStamp, RootedDirectory};
use super::maintenance_budget::CleanupMemory;
use super::maintenance_controls::{self, PublisherBinding, RootPublicationLease};
use crate::error::NativeError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub(crate) const ORIGIN_NAME: &[u8] = b".entry-origin";
pub(crate) const PAYLOAD_NAME: &[u8] = b"partition.json";
const FRAME: usize = 512;
const MAX_BYTES: usize = 4 * FRAME;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginPhase {
    Bound,
    Writing,
    Complete,
    Published,
}
impl OriginPhase {
    fn number(self) -> u8 {
        match self {
            Self::Bound => 0,
            Self::Writing => 1,
            Self::Complete => 2,
            Self::Published => 3,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OriginScope {
    pub(crate) root: FileIdentity,
    pub(crate) coordination: FileIdentity,
    pub(crate) cohort: FileIdentity,
    pub(crate) publication_binding: FileIdentity,
    pub(crate) selector: FileIdentity,
    pub(crate) incarnation: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OriginFacts {
    pub(crate) scope: OriginScope,
    pub(crate) key: [u8; 32],
    pub(crate) operation_nonce: [u8; 32],
    pub(crate) directory: FileIdentity,
    pub(crate) payload: FileIdentity,
    pub(crate) receipt: FileIdentity,
    pub(crate) directory_created: bool,
    pub(crate) payload_created: bool,
    pub(crate) phase: OriginPhase,
    pub(crate) byte_len: u64,
    pub(crate) payload_sha256: [u8; 32],
    pub(crate) torn_tail: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginObservation {
    Missing,
    Unknown(&'static str),
    Observed(OriginFacts),
}
fn invalid(reason: &str) -> NativeError {
    NativeError::InvalidInput(reason.into())
}
fn stamp(file: &File) -> Result<FixedFileStamp, NativeError> {
    FixedFileStamp::capture(file, FixedControlKind::File)
        .map_err(|_| invalid("artifact_origin_file_changed_or_aliased"))
}
fn recheck(s: FixedFileStamp, file: &File) -> Result<(), NativeError> {
    s.recheck(file)
        .map_err(|_| invalid("artifact_origin_source_changed"))
}
fn identity(bytes: &mut [u8], id: FileIdentity) {
    let (a, b) = id.fixed_words();
    bytes[..8].copy_from_slice(&a.to_le_bytes());
    bytes[8..16].copy_from_slice(&b.to_le_bytes());
}
fn read_identity(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    )
}
fn encode(f: OriginFacts) -> [u8; FRAME] {
    let mut b = [0; FRAME];
    b[..8].copy_from_slice(b"CGORIG01");
    b[8..16].copy_from_slice(&1u64.to_le_bytes());
    b[16..24].copy_from_slice(&1u64.to_le_bytes());
    b[24] = f.phase.number();
    b[25] = u8::from(f.directory_created) | (u8::from(f.payload_created) << 1);
    b[32..64].copy_from_slice(&f.scope.incarnation);
    b[64..96].copy_from_slice(&f.key);
    b[96..128].copy_from_slice(&f.operation_nonce);
    for (offset, id) in [
        (128, f.scope.root),
        (144, f.scope.coordination),
        (160, f.scope.cohort),
        (176, f.scope.publication_binding),
        (192, f.scope.selector),
        (208, f.directory),
        (224, f.payload),
        (240, f.receipt),
    ] {
        identity(&mut b[offset..offset + 16], id);
    }
    b[256..264].copy_from_slice(&f.byte_len.to_le_bytes());
    b[264..296].copy_from_slice(&f.payload_sha256);
    let digest = Sha256::digest(&b[..480]);
    b[480..].copy_from_slice(&digest);
    b
}
fn decode(b: &[u8; FRAME]) -> Option<OriginFacts> {
    if &b[..8] != b"CGORIG01"
        || u64::from_le_bytes(b[8..16].try_into().ok()?) != 1
        || u64::from_le_bytes(b[16..24].try_into().ok()?) != 1
        || b[24] > 3
        || b[25] > 3
        || b[26..32].iter().chain(b[296..480].iter()).any(|v| *v != 0)
        || Sha256::digest(&b[..480]).as_slice() != &b[480..]
    {
        return None;
    }
    let phase = [
        OriginPhase::Bound,
        OriginPhase::Writing,
        OriginPhase::Complete,
        OriginPhase::Published,
    ][usize::from(b[24])];
    let byte_len = u64::from_le_bytes(b[256..264].try_into().ok()?);
    let payload_sha256 = b[264..296].try_into().ok()?;
    if phase.number() < 2 && (byte_len != 0 || payload_sha256 != [0; 32]) {
        return None;
    }
    if b[96..128].iter().all(|v| *v == 0) || b[32..64].iter().all(|v| *v == 0) {
        return None;
    }
    Some(OriginFacts {
        scope: OriginScope {
            root: read_identity(&b[128..144]),
            coordination: read_identity(&b[144..160]),
            cohort: read_identity(&b[160..176]),
            publication_binding: read_identity(&b[176..192]),
            selector: read_identity(&b[192..208]),
            incarnation: b[32..64].try_into().ok()?,
        },
        key: b[64..96].try_into().ok()?,
        operation_nonce: b[96..128].try_into().ok()?,
        directory: read_identity(&b[208..224]),
        payload: read_identity(&b[224..240]),
        receipt: read_identity(&b[240..256]),
        directory_created: b[25] & 1 != 0,
        payload_created: b[25] & 2 != 0,
        phase,
        byte_len,
        payload_sha256,
        torn_tail: false,
    })
}
fn scope(lease: &RootPublicationLease<'_, '_>) -> OriginScope {
    OriginScope {
        root: lease.binding().root,
        coordination: lease.binding().coordination,
        cohort: lease.root_cohort_identity(),
        publication_binding: lease.publication_binding_identity(),
        selector: lease.binding().selector,
        incarnation: lease.binding().incarnation,
    }
}

/// Created exclusively by the retained actual producer publication lease. Its
/// constructor never adopts an existing receipt and its log is at most2048B.
pub(crate) struct OriginWriter {
    file: File,
    facts: OriginFacts,
    usable: bool,
}
impl OriginWriter {
    pub(crate) fn create(
        lease: &RootPublicationLease<'_, '_>,
        directory: &RootedDirectory,
        payload: &File,
        key: [u8; 32],
        directory_created: bool,
        payload_created: bool,
    ) -> Result<Self, NativeError> {
        if !lease.revalidate_existing()? {
            return Err(invalid("artifact_origin_scope_changed"));
        }
        let payload_stamp = stamp(payload)?;
        let named = directory.open_readonly_file(PAYLOAD_NAME)?;
        if locks::file_identity(&named)? != payload_stamp.identity() {
            return Err(invalid("artifact_origin_payload_changed"));
        }
        if payload_created && payload_stamp.len() != 0 {
            return Err(invalid("artifact_origin_created_payload_not_empty"));
        }
        let mut operation_nonce = [0; 32];
        getrandom::fill(&mut operation_nonce)
            .map_err(|_| invalid("artifact_origin_nonce_unavailable"))?;
        let file = directory.create_new(ORIGIN_NAME)?;
        let receipt = locks::file_identity(&file)?;
        let facts = OriginFacts {
            scope: scope(lease),
            key,
            operation_nonce,
            directory: directory.identity(),
            payload: payload_stamp.identity(),
            receipt,
            directory_created,
            payload_created,
            phase: OriginPhase::Bound,
            byte_len: 0,
            payload_sha256: [0; 32],
            torn_tail: false,
        };
        let mut writer = Self {
            file,
            facts,
            usable: true,
        };
        if writer
            .file
            .write_all(&encode(facts))
            .and_then(|_| writer.file.sync_all())
            .and_then(|_| directory.sync_namespace())
            .is_err()
        {
            // Retain exact creation identity even if optional persistence failed.
            // The primary producer may safely carry only its own created leaf;
            // later observation treats this unbound/torn record as unknown.
            writer.usable = false;
        }
        Ok(writer)
    }
    pub(crate) fn receipt_identity(&self) -> FileIdentity {
        self.facts.receipt
    }
    pub(crate) fn writing(&mut self) -> Result<(), NativeError> {
        self.append(OriginPhase::Writing, 0, [0; 32])
    }
    pub(crate) fn complete(
        &mut self,
        directory: &RootedDirectory,
        payload: &File,
        len: u64,
        digest: [u8; 32],
    ) -> Result<(), NativeError> {
        if !self.usable || self.facts.phase != OriginPhase::Writing {
            return Err(invalid("artifact_origin_incomplete_writer"));
        }
        let (actual_len, actual_digest) = hash_payload(payload)?;
        if directory.identity() != self.facts.directory
            || locks::file_identity(&directory.open_readonly_file(PAYLOAD_NAME)?)?
                != self.facts.payload
            || locks::file_identity(payload)? != self.facts.payload
            || actual_len != len
            || actual_digest != digest
        {
            return Err(invalid("artifact_origin_written_bytes_differ"));
        }
        directory.sync_namespace()?;
        self.append(OriginPhase::Complete, len, digest)
    }
    pub(crate) fn published(
        &mut self,
        lease: &RootPublicationLease<'_, '_>,
        directory: &RootedDirectory,
    ) -> Result<(), NativeError> {
        if !lease.revalidate_existing()?
            || scope(lease) != self.facts.scope
            || directory.identity() != self.facts.directory
            || locks::file_identity(&directory.open_readonly_file(PAYLOAD_NAME)?)?
                != self.facts.payload
            || locks::file_identity(&directory.open_readonly_file(ORIGIN_NAME)?)?
                != self.facts.receipt
        {
            return Err(invalid("artifact_origin_publication_changed"));
        }
        self.append(
            OriginPhase::Published,
            self.facts.byte_len,
            self.facts.payload_sha256,
        )
    }
    fn append(
        &mut self,
        phase: OriginPhase,
        len: u64,
        digest: [u8; 32],
    ) -> Result<(), NativeError> {
        if !self.usable
            || phase.number() != self.facts.phase.number() + 1
            || self.file.metadata()?.len() != u64::from(phase.number()) * FRAME as u64
        {
            return Err(invalid("artifact_origin_nonmonotone_phase"));
        }
        self.usable = false;
        let facts = OriginFacts {
            phase,
            byte_len: len,
            payload_sha256: digest,
            ..self.facts
        };
        self.file.write_all(&encode(facts))?;
        self.file.sync_all()?;
        self.facts = facts;
        self.usable = true;
        Ok(())
    }
}
/// Hash actual stable file bytes. Producer callers are outside janitor budget;
/// observation callers precharge this fixed buffer/state in the shared account.
pub(crate) fn hash_payload(file: &File) -> Result<(u64, [u8; 32]), NativeError> {
    let before = stamp(file)?;
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(0))?;
    let mut bytes = [0; 8192];
    let mut hash = Sha256::new();
    let mut len = 0u64;
    loop {
        let n = cursor.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        len = len
            .checked_add(n as u64)
            .ok_or_else(|| invalid("artifact_origin_length_overflow"))?;
        hash.update(&bytes[..n]);
    }
    recheck(before, file)?;
    if len != before.len() {
        return Err(invalid("artifact_origin_length_changed"));
    }
    Ok((len, hash.finalize().into()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Cohort {
    version: u32,
    root: CohortIdentity,
    coordination: CohortIdentity,
    coordination_incarnation: [u8; 32],
    origin: CohortOrigin,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CohortIdentity {
    volume: u64,
    index: u64,
}
impl CohortIdentity {
    fn physical(&self) -> FileIdentity {
        FileIdentity::from_fixed_words(self.volume, self.index)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum CohortOrigin {
    GuardedCreation,
    ControlledMigration,
}
fn read_exact<const N: usize>(file: &File) -> Result<[u8; N], NativeError> {
    let before = stamp(file)?;
    if before.len() != N as u64 {
        return Err(invalid("artifact_origin_control_size"));
    }
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(0))?;
    let mut bytes = [0; N];
    cursor.read_exact(&mut bytes)?;
    recheck(before, file)?;
    Ok(bytes)
}
fn observe_scope(root: &RootedDirectory) -> Result<Option<OriginScope>, NativeError> {
    let coordination = root.open_readonly_file(b".use.lock")?;
    let coordination_stamp = stamp(&coordination)?;
    let Some(incarnation) = locks::read_coordination_incarnation(&coordination)? else {
        return Ok(None);
    };
    let cohort = root.open_readonly_file(b".use-cohort.json")?;
    let cohort_stamp = stamp(&cohort)?;
    if cohort_stamp.len() > 1024 {
        return Ok(None);
    }
    let mut bytes = [0; 1024];
    let mut cursor = &cohort;
    cursor.read_exact(&mut bytes[..cohort_stamp.len() as usize])?;
    let Ok(record) = serde_json::from_slice::<Cohort>(&bytes[..cohort_stamp.len() as usize]) else {
        return Ok(None);
    };
    let _ = record.origin;
    if record.version != 2
        || record.root.physical() != root.identity()
        || record.coordination.physical() != coordination_stamp.identity()
        || record.coordination_incarnation != incarnation
    {
        return Ok(None);
    }
    let binding_file = root.open_readonly_file(maintenance_controls::BINDING_NAME)?;
    let binding = PublisherBinding::decode(&read_exact::<{ maintenance_controls::BINDING_BYTES }>(
        &binding_file,
    )?)
    .map_err(|_| invalid("artifact_origin_publisher_binding_unknown"))?;
    if binding.root != root.identity()
        || binding.coordination != coordination_stamp.identity()
        || binding.incarnation != incarnation
    {
        return Ok(None);
    }
    if locks::file_identity(&root.open_readonly_file(maintenance_controls::SELECTOR_NAME)?)?
        != binding.selector
    {
        return Ok(None);
    }
    recheck(coordination_stamp, &coordination)?;
    recheck(cohort_stamp, &cohort)?;
    Ok(Some(OriginScope {
        root: root.identity(),
        coordination: coordination_stamp.identity(),
        cohort: cohort_stamp.identity(),
        publication_binding: locks::file_identity(&binding_file)?,
        selector: binding.selector,
        incarnation,
    }))
}
/// Noncreating fixed-size observation. An incomplete reused receipt NEVER
/// establishes creation of the existing payload. Complete facts require actual
/// unchanged byte integrity. The caller still proves references and authority.
pub(crate) fn observe_artifact_origin(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
) -> Result<OriginObservation, NativeError> {
    let floor = std::mem::size_of::<OriginFacts>() * 5
        + MAX_BYTES
        + FRAME
        + 1024
        + 8192
        + std::mem::size_of::<FixedFileStamp>() * 8
        + std::mem::size_of::<std::fs::Metadata>() * 8
        + std::mem::size_of::<RootedDirectory>() * 4
        + 2048;
    let _floor = memory
        .reserve(floor)
        .map_err(|_| invalid("artifact_origin_memory_limit"))?;
    let scope = match observe_scope(root) {
        Ok(Some(scope)) => scope,
        _ => return Ok(OriginObservation::Unknown("root_origin_scope_unverified")),
    };
    let file = match directory.open_readonly_file(ORIGIN_NAME) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(OriginObservation::Missing)
        }
        Err(error) => return Err(error.into()),
    };
    let before = stamp(&file)?;
    if before.len() < FRAME as u64 || before.len() > MAX_BYTES as u64 {
        return Ok(OriginObservation::Unknown(
            "origin_size_or_unbound_creation",
        ));
    }
    let mut bytes = [0; MAX_BYTES];
    let len = before.len() as usize;
    let mut cursor = &file;
    cursor.read_exact(&mut bytes[..len])?;
    let full = len / FRAME;
    let mut latest: Option<OriginFacts> = None;
    for i in 0..full {
        let image: &[u8; FRAME] = bytes[i * FRAME..(i + 1) * FRAME].try_into().unwrap();
        let Some(facts) = decode(image) else {
            return Ok(OriginObservation::Unknown("origin_frame_invalid"));
        };
        if usize::from(facts.phase.number()) != i
            || facts.scope != scope
            || facts.key != key
            || facts.directory != directory.identity()
            || facts.payload != locks::file_identity(payload)?
            || facts.receipt != before.identity()
        {
            return Ok(OriginObservation::Unknown("origin_binding_changed"));
        }
        if let Some(old) = latest {
            let mut comparable = facts;
            comparable.phase = old.phase;
            comparable.byte_len = old.byte_len;
            comparable.payload_sha256 = old.payload_sha256;
            if comparable != old {
                return Ok(OriginObservation::Unknown("origin_operation_changed"));
            }
            if i == 3
                && (facts.byte_len != old.byte_len || facts.payload_sha256 != old.payload_sha256)
            {
                return Ok(OriginObservation::Unknown("origin_completion_changed"));
            }
        }
        latest = Some(facts);
    }
    let mut facts = latest.unwrap();
    let tail = len % FRAME;
    if tail != 0 {
        if full >= 4 {
            return Ok(OriginObservation::Unknown("origin_extra_bytes"));
        }
        let mut next = facts;
        next.phase = [
            OriginPhase::Bound,
            OriginPhase::Writing,
            OriginPhase::Complete,
            OriginPhase::Published,
        ][full];
        if full == 2 {
            let (n, d) = hash_payload(payload)?;
            next.byte_len = n;
            next.payload_sha256 = d;
        }
        if bytes[full * FRAME..len] != encode(next)[..tail] {
            return Ok(OriginObservation::Unknown("origin_torn_unknown_bytes"));
        }
        facts.torn_tail = true;
    }
    if facts.phase.number() >= 2 {
        let (len, digest) = hash_payload(payload)?;
        if len != facts.byte_len || digest != facts.payload_sha256 {
            return Ok(OriginObservation::Unknown(
                "origin_payload_integrity_changed",
            ));
        }
    }
    recheck(before, &file)?;
    if locks::file_identity(&directory.open_readonly_file(ORIGIN_NAME)?)? != facts.receipt
        || locks::file_identity(&directory.open_readonly_file(PAYLOAD_NAME)?)? != facts.payload
    {
        return Ok(OriginObservation::Unknown(
            "origin_named_incarnation_changed",
        ));
    }
    if observe_scope(root)? != Some(scope) {
        return Ok(OriginObservation::Unknown("origin_scope_changed"));
    }
    Ok(OriginObservation::Observed(facts))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dc_origin_actual_creation_frames_interrupted_publication_and_torn_prefix() {
        let path = std::env::temp_dir().join(format!(
            "dc-origin-real-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        use super::super::{
            layout::{create_fresh_reference_root, ManagedLayout},
            locks::{open_locked, LockMode},
            maintenance_ownership::{initialize_fresh_reference_domain, ReferenceDomain},
        };
        let layout = ManagedLayout::new(path.join("storage"));
        let fresh = create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        let mutation = super::super::maintenance_ownership::ReferenceMutation::begin_producing(
            &layout.artifacts_root(),
        )
        .unwrap();
        let mut capability = mutation
            .artifact_root_producer_capability()
            .unwrap()
            .unwrap();
        let lease = maintenance_controls::try_begin_root_publication(&mut capability)
            .unwrap()
            .unwrap();
        let root = lease.directory();
        let directory = root.create_directory(b"actual-entry").unwrap();
        let mut payload = directory.create_new(PAYLOAD_NAME).unwrap();
        let key = [17; 32];
        let mut writer =
            OriginWriter::create(&lease, &directory, &payload, key, true, true).unwrap();
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        let observe = |payload: &File| {
            observe_artifact_origin(root, &directory, payload, key, &memory).unwrap()
        };
        assert!(matches!(
            observe(&payload),
            OriginObservation::Observed(OriginFacts {
                phase: OriginPhase::Bound,
                payload_created: true,
                ..
            })
        ));
        writer.writing().unwrap();
        payload
            .write_all(b"actual accepted producer bytes")
            .unwrap();
        payload.sync_all().unwrap();
        let (len, digest) = hash_payload(&payload).unwrap();
        writer.complete(&directory, &payload, len, digest).unwrap();
        assert!(matches!(
            observe(&payload),
            OriginObservation::Observed(OriginFacts {
                phase: OriginPhase::Complete,
                ..
            })
        ));
        let next = OriginFacts {
            phase: OriginPhase::Published,
            ..writer.facts
        };
        writer.file.write_all(&encode(next)[..31]).unwrap();
        writer.file.sync_all().unwrap();
        assert!(matches!(
            observe(&payload),
            OriginObservation::Observed(OriginFacts {
                phase: OriginPhase::Complete,
                torn_tail: true,
                ..
            })
        ));
        writer.file.write_all(b"bad trailing bytes").unwrap();
        writer.file.sync_all().unwrap();
        assert!(matches!(observe(&payload), OriginObservation::Unknown(_)));
        writer.file.set_len(3 * FRAME as u64).unwrap();
        writer.file.seek(SeekFrom::End(0)).unwrap();
        writer.published(&lease, &directory).unwrap();
        assert!(matches!(
            observe(&payload),
            OriginObservation::Observed(OriginFacts {
                phase: OriginPhase::Published,
                ..
            })
        ));
        payload.seek(SeekFrom::Start(0)).unwrap();
        payload.write_all(b"X").unwrap();
        payload.sync_all().unwrap();
        assert_eq!(
            observe(&payload),
            OriginObservation::Unknown("origin_payload_integrity_changed")
        );
        drop(writer);
        drop(payload);
        drop(directory);
        drop(lease);
        drop(capability);
        drop(mutation);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn dc_origin_frames_fail_closed_on_wrong_key_scope_identity_and_extra_bytes() {
        let id = FileIdentity::from_fixed_words(1, 2);
        let facts = OriginFacts {
            scope: OriginScope {
                root: id,
                coordination: id,
                cohort: id,
                publication_binding: id,
                selector: id,
                incarnation: [1; 32],
            },
            key: [2; 32],
            operation_nonce: [3; 32],
            directory: id,
            payload: id,
            receipt: id,
            directory_created: true,
            payload_created: true,
            phase: OriginPhase::Bound,
            byte_len: 0,
            payload_sha256: [0; 32],
            torn_tail: false,
        };
        let image = encode(facts);
        assert_eq!(decode(&image), Some(facts));
        for offset in [
            0, 8, 16, 24, 25, 32, 64, 96, 128, 208, 224, 240, 264, 480, 511,
        ] {
            let mut corrupt = image;
            corrupt[offset] ^= 0x80;
            assert_eq!(decode(&corrupt), None);
        }
    }
}
