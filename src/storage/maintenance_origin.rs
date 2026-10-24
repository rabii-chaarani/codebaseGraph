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
    pub(crate) payload_nonce: [u8; 32],
    pub(crate) payload_stamp: Option<FixedFileStamp>,
    pub(crate) torn_tail: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Fixed inline protocol/lease state: boxing would add a separately accounted allocation.
#[allow(clippy::large_enum_variant)]
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
    b[8..16].copy_from_slice(&2u64.to_le_bytes());
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
    if let Some(stamp) = f.payload_stamp {
        stamp.encode_checkpoint((&mut b[296..376]).try_into().unwrap());
    }
    b[376..408].copy_from_slice(&f.payload_nonce);
    let digest = Sha256::digest(&b[..480]);
    b[480..].copy_from_slice(&digest);
    b
}
fn decode(b: &[u8; FRAME]) -> Option<OriginFacts> {
    if &b[..8] != b"CGORIG01"
        || u64::from_le_bytes(b[8..16].try_into().ok()?) != 2
        || u64::from_le_bytes(b[16..24].try_into().ok()?) != 1
        || b[24] > 3
        || b[25] > 3
        || b[26..32].iter().chain(b[408..480].iter()).any(|v| *v != 0)
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
    let payload_nonce: [u8; 32] = b[376..408].try_into().ok()?;
    if payload_nonce == [0; 32] {
        return None;
    }
    let payload_stamp = if phase.number() < 2 {
        if b[296..376].iter().any(|b| *b != 0) {
            return None;
        }
        None
    } else {
        if b[320] != 0 {
            return None;
        }
        let stamp = FixedFileStamp::decode_checkpoint(b[296..376].try_into().ok()?).ok()?;
        if stamp.identity() != read_identity(&b[224..240]) || stamp.len() != byte_len {
            return None;
        }
        Some(stamp)
    };
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
        payload_nonce,
        payload_stamp,
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
        if operation_nonce == [0; 32] {
            return Err(invalid("artifact_origin_zero_nonce"));
        }
        let payload_nonce = match locks::created_file_nonce(payload)? {
            Some(nonce) if !payload_created && nonce != [0; 32] => nonce,
            Some(_) => return Err(invalid("artifact_origin_existing_creation_marker")),
            None => {
                // Existing byte-equal payload attestation may add only this
                // immutable marker. Flags remain false; it claims no creation.
                let writable = directory.open_existing(PAYLOAD_NAME)?;
                payload_stamp
                    .recheck(&writable)
                    .map_err(|_| invalid("artifact_origin_attestation_source_changed"))?;
                locks::set_created_file_nonce(&writable, &operation_nonce)?;
                if locks::created_file_nonce(payload)? != Some(operation_nonce) {
                    return Err(invalid("artifact_origin_marker_readback_changed"));
                }
                operation_nonce
            }
        };
        let file = directory.create_new(ORIGIN_NAME)?;
        // Only this exclusive creator may bind the physical receipt birth to
        // the operation recorded in its frames. Never repair existing files.
        let receipt_stamped = locks::set_created_file_nonce(&file, &operation_nonce).is_ok()
            && locks::created_file_nonce(&file).ok().flatten() == Some(operation_nonce);
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
            payload_nonce,
            payload_stamp: None,
            torn_tail: false,
        };
        let mut writer = Self {
            file,
            facts,
            usable: receipt_stamped,
        };
        if !writer.usable
            || writer
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
        self.append(OriginPhase::Writing, 0, [0; 32], None)
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
        let before = stamp(payload)?;
        let (actual_len, actual_digest) = hash_payload(payload)?;
        recheck(before, payload)?;
        if locks::created_file_nonce(payload)? != Some(self.facts.payload_nonce) {
            return Err(invalid("artifact_origin_payload_nonce_changed"));
        }
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
        self.append(OriginPhase::Complete, len, digest, Some(before))
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
        let payload = directory.open_readonly_file(PAYLOAD_NAME)?;
        let before = stamp(&payload)?;
        if locks::created_file_nonce(&payload)? != Some(self.facts.payload_nonce) {
            return Err(invalid("artifact_origin_publication_nonce_changed"));
        }
        if self.facts.payload_stamp != Some(before) {
            let (len, digest) = hash_payload(&payload)?;
            recheck(before, &payload)?;
            if len != self.facts.byte_len || digest != self.facts.payload_sha256 {
                return Err(invalid("artifact_origin_publication_bytes_changed"));
            }
        }
        self.append(
            OriginPhase::Published,
            self.facts.byte_len,
            self.facts.payload_sha256,
            Some(before),
        )
    }
    fn append(
        &mut self,
        phase: OriginPhase,
        len: u64,
        digest: [u8; 32],
        payload_stamp: Option<FixedFileStamp>,
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
            payload_stamp,
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
    hash_payload_checked(file, &|| false)
}
fn hash_payload_checked(
    file: &File,
    cancel: &dyn Fn() -> bool,
) -> Result<(u64, [u8; 32]), NativeError> {
    let before = stamp(file)?;
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(0))?;
    let mut bytes = [0; 8192];
    let mut hash = Sha256::new();
    let mut len = 0u64;
    loop {
        if cancel() {
            return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
        }
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
/// Existing-only fixed root/cohort/binding facts. No producer capability,
/// reference completeness, lifecycle or mutation authority is returned.
pub(crate) fn observe_artifact_root_scope(
    root: &RootedDirectory,
    memory: &CleanupMemory,
) -> Result<Option<OriginScope>, NativeError> {
    let _floor = memory
        .reserve(
            8192 + std::mem::size_of::<OriginScope>() * 2
                + std::mem::size_of::<FixedFileStamp>() * 8,
        )
        .map_err(|_| invalid("artifact_scope_memory_limit"))?;
    observe_scope(root)
}
/// Noncreating fixed-size observation. An incomplete reused receipt NEVER
/// establishes creation of the existing payload. Complete facts require actual
/// unchanged byte integrity. The caller still proves references and authority.
#[cfg(test)]
pub(crate) fn observe_artifact_origin(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
) -> Result<OriginObservation, NativeError> {
    observe_origin(root, directory, payload, key, memory, true, &|| false)
}
/// Read-only producer-restoration evidence for a retained, originally created
/// key directory whose payload is absent. This does not verify old payload
/// completion, loader usability, references, or any deletion authority.
pub(crate) fn observe_published_origin_without_payload(
    lease: &RootPublicationLease<'_, '_>,
    root: &RootedDirectory,
    directory: &RootedDirectory,
    key: [u8; 32],
) -> Result<bool, NativeError> {
    if !lease.revalidate_existing()? || observe_scope(root)? != Some(scope(lease)) {
        return Ok(false);
    }
    match directory.open_readonly_file(PAYLOAD_NAME) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Ok(false),
    }
    let file = match directory.open_readonly_file(ORIGIN_NAME) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let before = stamp(&file)?;
    if before.len() != MAX_BYTES as u64 {
        return Ok(false);
    }
    let bytes = read_exact::<MAX_BYTES>(&file)?;
    let mut previous: Option<OriginFacts> = None;
    for (index, image) in bytes.chunks_exact(FRAME).enumerate() {
        let Some(facts) = decode(image.try_into().unwrap()) else {
            return Ok(false);
        };
        if usize::from(facts.phase.number()) != index
            || facts.scope != scope(lease)
            || facts.key != key
            || facts.directory != directory.identity()
            || facts.receipt != before.identity()
            || !facts.directory_created
            || !facts.payload_created
            || locks::created_file_nonce(&file)? != Some(facts.operation_nonce)
        {
            return Ok(false);
        }
        if let Some(old) = previous {
            let mut comparable = facts;
            comparable.phase = old.phase;
            comparable.byte_len = old.byte_len;
            comparable.payload_sha256 = old.payload_sha256;
            comparable.payload_stamp = old.payload_stamp;
            if comparable != old
                || (index == 3
                    && (facts.byte_len != old.byte_len
                        || facts.payload_sha256 != old.payload_sha256
                        || facts.payload_stamp != old.payload_stamp))
            {
                return Ok(false);
            }
        }
        previous = Some(facts);
    }
    let facts = previous.unwrap();
    recheck(before, &file)?;
    if locks::file_identity(&directory.open_readonly_file(ORIGIN_NAME)?)? != before.identity()
        || locks::created_file_nonce(&file)? != Some(facts.operation_nonce)
        || !lease.revalidate_existing()?
        || observe_scope(root)? != Some(scope(lease))
    {
        return Ok(false);
    }
    Ok(matches!(directory.open_readonly_file(PAYLOAD_NAME),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound))
}

/// Facts for a journal-bound detached payload. The caller must independently
/// prove the exact private address and incarnation from its durable journal.
/// This function provides no publication, reference or deletion authority.
pub(crate) fn observe_detached_artifact_origin(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<OriginObservation, NativeError> {
    observe_origin(root, directory, payload, key, memory, false, cancel)
}
pub(crate) fn observe_artifact_origin_checked(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
    cancel: &dyn Fn() -> bool,
) -> Result<OriginObservation, NativeError> {
    observe_origin(root, directory, payload, key, memory, true, cancel)
}
fn observe_origin(
    root: &RootedDirectory,
    directory: &RootedDirectory,
    payload: &File,
    key: [u8; 32],
    memory: &CleanupMemory,
    require_named: bool,
    cancel: &dyn Fn() -> bool,
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
            comparable.payload_stamp = old.payload_stamp;
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
    if locks::created_file_nonce(payload)? != Some(facts.payload_nonce) {
        return Ok(OriginObservation::Unknown(
            "origin_payload_incarnation_changed",
        ));
    }
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
            let (n, d) = hash_payload_checked(payload, cancel)?;
            next.byte_len = n;
            next.payload_sha256 = d;
            next.payload_stamp = Some(stamp(payload)?);
        }
        if bytes[full * FRAME..len] != encode(next)[..tail] {
            return Ok(OriginObservation::Unknown("origin_torn_unknown_bytes"));
        }
        facts.torn_tail = true;
    }
    if facts.phase.number() >= 2 {
        let (len, digest) = hash_payload_checked(payload, cancel)?;
        if len != facts.byte_len || digest != facts.payload_sha256 {
            return Ok(OriginObservation::Unknown(
                "origin_payload_integrity_changed",
            ));
        }
    }
    recheck(before, &file)?;
    if locks::file_identity(&directory.open_readonly_file(ORIGIN_NAME)?)? != facts.receipt
        || (require_named
            && locks::file_identity(&directory.open_readonly_file(PAYLOAD_NAME)?)? != facts.payload)
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
        std::fs::create_dir_all(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let layout = ManagedLayout::new(path.join("storage"));
        let fresh = create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
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
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
        drop(writer);
        drop(payload);
        drop(directory);
        drop(lease);
        // Consume this non-owning proof before releasing or checking its lender.
        #[allow(clippy::drop_non_drop)]
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
            payload_nonce: [4; 32],
            payload_stamp: None,
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

pub(crate) const WORKSPACE_ORIGIN_FRAME_BYTES: usize = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceOriginPhase {
    Bound,
    Complete,
}
/// Decoder facts only; callers separately match every field against an actual
/// admitted workspace scope, output nonce, parent and receipt incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceOriginFacts {
    pub(crate) directory: bool,
    pub(crate) control: bool,
    pub(crate) fifo: bool,
    pub(crate) intended_name: [u8; 255],
    pub(crate) intended_name_len: u8,
    pub(crate) binding: maintenance_controls::WorkspaceOutputBinding,
    pub(crate) receipt: FileIdentity,
    pub(crate) phase: WorkspaceOriginPhase,
    pub(crate) byte_len: u64,
    pub(crate) payload_sha256: [u8; 32],
}
fn workspace_frame(f: WorkspaceOriginFacts) -> [u8; WORKSPACE_ORIGIN_FRAME_BYTES] {
    let mut bytes = [0; WORKSPACE_ORIGIN_FRAME_BYTES];
    bytes[..8].copy_from_slice(b"CGWOUT01");
    bytes[8] = u8::from(f.phase == WorkspaceOriginPhase::Complete);
    bytes[9] = f.binding.name_len;
    bytes[10] = if f.fifo {
        3
    } else if f.control {
        2
    } else {
        u8::from(f.directory)
    };
    bytes[16..48].copy_from_slice(&f.binding.incarnation);
    bytes[48..80].copy_from_slice(&f.binding.operation);
    for (index, id) in [
        f.binding.root,
        f.binding.coordination,
        f.binding.registration,
        f.binding.publication_binding,
        f.binding.registry,
        f.binding.parent,
        f.binding.output,
        f.receipt,
    ]
    .iter()
    .enumerate()
    {
        let at = 80 + index * 16;
        identity(&mut bytes[at..at + 16], *id);
    }
    bytes[208..216].copy_from_slice(&f.byte_len.to_le_bytes());
    bytes[216..248].copy_from_slice(&f.payload_sha256);
    bytes[248..503].copy_from_slice(&f.binding.name);
    bytes[503] = f.intended_name_len;
    bytes[504..759].copy_from_slice(&f.intended_name);
    let hash = Sha256::digest(&bytes[..992]);
    bytes[992..].copy_from_slice(&hash);
    bytes
}
fn workspace_fact(bytes: &[u8; WORKSPACE_ORIGIN_FRAME_BYTES]) -> Option<WorkspaceOriginFacts> {
    let len = usize::from(bytes[9]);
    if &bytes[..8] != b"CGWOUT01"
        || bytes[8] > 1
        || len == 0
        || bytes[10] > 3
        || matches!(&bytes[248..248 + len], b"." | b"..")
        || (bytes[10] == 3 && &bytes[248..248 + len] != b"writers.fifo")
        || bytes[11..16]
            .iter()
            .chain(bytes[759..992].iter())
            .any(|b| *b != 0)
        || bytes[248 + len..503].iter().any(|b| *b != 0)
        || bytes[248..248 + len]
            .iter()
            .any(|b| *b == 0 || *b == b'/' || *b == b'\\')
        || bytes[992..] != Sha256::digest(&bytes[..992])[..]
        || bytes[16..48].iter().all(|b| *b == 0)
        || bytes[48..80].iter().all(|b| *b == 0)
    {
        return None;
    }
    let intended_len = bytes[503] as usize;
    if (bytes[10] != 2 && bytes[503..759].iter().any(|b| *b != 0))
        || (bytes[10] == 2
            && (intended_len == 0
                || matches!(&bytes[504..504 + intended_len], b"." | b"..")
                || bytes[504 + intended_len..759].iter().any(|b| *b != 0)
                || bytes[504..504 + intended_len]
                    .iter()
                    .any(|b| matches!(*b, 0 | b'/' | b'\\'))))
    {
        return None;
    }
    let phase = if bytes[8] == 0 {
        WorkspaceOriginPhase::Bound
    } else {
        WorkspaceOriginPhase::Complete
    };
    let byte_len = u64::from_le_bytes(bytes[208..216].try_into().ok()?);
    let payload_sha256 = bytes[216..248].try_into().ok()?;
    if (phase == WorkspaceOriginPhase::Bound && (byte_len != 0 || payload_sha256 != [0; 32]))
        || (bytes[10] != 0 && phase != WorkspaceOriginPhase::Bound)
    {
        return None;
    }
    Some(WorkspaceOriginFacts {
        directory: bytes[10] == 1,
        control: bytes[10] == 2,
        fifo: bytes[10] == 3,
        intended_name: bytes[504..759].try_into().ok()?,
        intended_name_len: bytes[503],
        binding: maintenance_controls::WorkspaceOutputBinding {
            root: read_identity(&bytes[80..96]),
            coordination: read_identity(&bytes[96..112]),
            registration: read_identity(&bytes[112..128]),
            publication_binding: read_identity(&bytes[128..144]),
            registry: read_identity(&bytes[144..160]),
            parent: read_identity(&bytes[160..176]),
            output: read_identity(&bytes[176..192]),
            incarnation: bytes[16..48].try_into().ok()?,
            operation: bytes[48..80].try_into().ok()?,
            name: bytes[248..503].try_into().ok()?,
            name_len: bytes[9],
        },
        receipt: read_identity(&bytes[192..208]),
        phase,
        byte_len,
        payload_sha256,
    })
}
fn workspace_bound(
    binding: maintenance_controls::WorkspaceOutputBinding,
    receipt: FileIdentity,
) -> WorkspaceOriginFacts {
    WorkspaceOriginFacts {
        directory: false,
        control: false,
        fifo: false,
        intended_name: [0; 255],
        intended_name_len: 0,
        binding,
        receipt,
        phase: WorkspaceOriginPhase::Bound,
        byte_len: 0,
        payload_sha256: [0; 32],
    }
}
/// Called by a real O_EXCL producer token BEFORE any payload bytes are written.
/// Failure is optional tracking failure, never permission to adopt a collision.
pub(crate) fn record_created_workspace_output(
    output: &maintenance_controls::WorkspaceOutputFile,
) -> Result<(), NativeError> {
    let mut receipt = output.create_origin()?;
    let bound = workspace_frame(workspace_bound(output.origin_binding(), receipt.identity()));
    receipt.write_all(&bound)?;
    receipt.sync_all()?;
    receipt.sync_parent()?;
    receipt.seek(SeekFrom::Start(0))?;
    let mut read = [0; WORKSPACE_ORIGIN_FRAME_BYTES];
    receipt.read_exact(&mut read)?;
    if read != bound || receipt.len()? != WORKSPACE_ORIGIN_FRAME_BYTES as u64 {
        return Err(invalid("workspace_creation_origin_readback_changed"));
    }
    Ok(())
}
/// An actual control creation binds only its temporary name and the producer's
/// single intended atomic-publication name, never arbitrary renamed aliases.
pub(crate) fn record_created_workspace_control(
    output: &maintenance_controls::WorkspaceCreatedControl,
) -> Result<(), NativeError> {
    let intended = output.intended_name();
    if intended.is_empty() || intended.len() > 255 {
        return Err(invalid("workspace_control_name_invalid"));
    }
    let mut receipt = output.create_origin()?;
    let mut facts = workspace_bound(output.origin_binding(), receipt.identity());
    facts.control = true;
    facts.intended_name_len = intended.len() as u8;
    facts.intended_name[..intended.len()].copy_from_slice(intended);
    let frame = workspace_frame(facts);
    receipt.write_all(&frame)?;
    receipt.sync_all()?;
    receipt.sync_parent()?;
    receipt.seek(SeekFrom::Start(0))?;
    let mut actual = [0; 1024];
    receipt.read_exact(&mut actual)?;
    if actual != frame || receipt.len()? != 1024 {
        return Err(invalid("workspace_control_origin_readback_changed"));
    }
    Ok(())
}
/// Creation-only FIFO proof. The producer holds the actual FIFO and its
/// registry pin; neither a name nor a decoded frame can construct this token.
#[cfg(unix)]
pub(crate) fn record_created_workspace_fifo(
    output: &maintenance_controls::WorkspaceCreatedFifo,
) -> Result<(), NativeError> {
    let mut receipt = output.create_origin()?;
    let mut facts = workspace_bound(output.origin_binding(), receipt.identity());
    facts.fifo = true;
    let frame = workspace_frame(facts);
    receipt.write_all(&frame)?;
    receipt.sync_all()?;
    receipt.sync_parent()?;
    receipt.seek(SeekFrom::Start(0))?;
    let mut actual = [0; WORKSPACE_ORIGIN_FRAME_BYTES];
    receipt.read_exact(&mut actual)?;
    if actual != frame || receipt.len()? != WORKSPACE_ORIGIN_FRAME_BYTES as u64 {
        return Err(invalid("workspace_fifo_origin_readback_changed"));
    }
    Ok(())
}
/// Creation fact for one genuine new directory. It never certifies ownership
/// of children; removal still requires matching incarnation and atomic-empty.
pub(crate) fn record_created_workspace_directory(
    output: &maintenance_controls::WorkspaceCreatedDirectory,
) -> Result<(), NativeError> {
    let mut receipt = output.create_origin()?;
    let mut facts = workspace_bound(output.origin_binding(), receipt.identity());
    facts.directory = true;
    let bound = workspace_frame(facts);
    receipt.write_all(&bound)?;
    receipt.sync_all()?;
    receipt.sync_parent()?;
    receipt.seek(SeekFrom::Start(0))?;
    let mut read = [0; WORKSPACE_ORIGIN_FRAME_BYTES];
    receipt.read_exact(&mut read)?;
    if read != bound || receipt.len()? != WORKSPACE_ORIGIN_FRAME_BYTES as u64 {
        return Err(invalid("workspace_directory_origin_readback_changed"));
    }
    Ok(())
}
/// The completed token retains the genuine creation, writer and reference
/// mutation through this call. A completed legacy/open-existing file cannot
/// construct the token. Only matching own partial completion prefixes resume.
pub(crate) fn record_completed_workspace_output(
    output: &maintenance_controls::WorkspaceCompletedOutput,
) -> Result<(), NativeError> {
    let mut receipt = match output.open_origin() {
        Ok(receipt) => receipt,
        Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut receipt = output.create_origin()?;
            let bound =
                workspace_frame(workspace_bound(output.origin_binding(), receipt.identity()));
            receipt.write_all(&bound)?;
            receipt.sync_all()?;
            receipt.sync_parent()?;
            receipt
        }
        Err(error) => return Err(error),
    };
    let bound = workspace_bound(output.origin_binding(), receipt.identity());
    let bound_bytes = workspace_frame(bound);
    let mut complete = bound;
    complete.phase = WorkspaceOriginPhase::Complete;
    complete.byte_len = output.len();
    complete.payload_sha256 = output.digest();
    let complete_bytes = workspace_frame(complete);
    let len = receipt.len()?;
    if !(WORKSPACE_ORIGIN_FRAME_BYTES as u64..=2 * WORKSPACE_ORIGIN_FRAME_BYTES as u64)
        .contains(&len)
    {
        return Err(invalid("workspace_origin_unknown_size_preserved"));
    }
    let len = len as usize;
    let mut read = [0; 2 * WORKSPACE_ORIGIN_FRAME_BYTES];
    receipt.seek(SeekFrom::Start(0))?;
    receipt.read_exact(&mut read[..len])?;
    let tail = len - WORKSPACE_ORIGIN_FRAME_BYTES;
    if read[..WORKSPACE_ORIGIN_FRAME_BYTES] != bound_bytes
        || read[WORKSPACE_ORIGIN_FRAME_BYTES..len] != complete_bytes[..tail]
    {
        return Err(invalid("workspace_origin_unknown_bytes_preserved"));
    }
    if tail != WORKSPACE_ORIGIN_FRAME_BYTES {
        receipt.seek(SeekFrom::Start(WORKSPACE_ORIGIN_FRAME_BYTES as u64))?;
        receipt.write_all(&complete_bytes)?;
        receipt.sync_all()?;
        receipt.sync_parent()?;
    }
    receipt.seek(SeekFrom::Start(WORKSPACE_ORIGIN_FRAME_BYTES as u64))?;
    receipt.read_exact(&mut read[..WORKSPACE_ORIGIN_FRAME_BYTES])?;
    if read[..WORKSPACE_ORIGIN_FRAME_BYTES] != complete_bytes
        || receipt.len()? != 2 * WORKSPACE_ORIGIN_FRAME_BYTES as u64
    {
        return Err(invalid("workspace_completion_origin_readback_changed"));
    }
    Ok(())
}
/// Strict fixed facts codec. A partial Complete prefix is accepted only after
/// the caller supplied the actual stable payload bytes' length and digest.
pub(crate) fn decode_workspace_output_origin(
    bytes: &[u8],
    receipt: FileIdentity,
    actual: Option<(u64, [u8; 32])>,
    memory: &CleanupMemory,
) -> Result<WorkspaceOriginFacts, NativeError> {
    let _floor = memory
        .reserve(
            3 * std::mem::size_of::<WorkspaceOriginFacts>()
                + 2 * WORKSPACE_ORIGIN_FRAME_BYTES
                + 512,
        )
        .map_err(|_| invalid("workspace_origin_memory_limit"))?;
    if !(WORKSPACE_ORIGIN_FRAME_BYTES..=2 * WORKSPACE_ORIGIN_FRAME_BYTES).contains(&bytes.len()) {
        return Err(invalid("workspace_origin_unknown_size"));
    }
    let bound = workspace_fact(bytes[..WORKSPACE_ORIGIN_FRAME_BYTES].try_into().unwrap())
        .ok_or_else(|| invalid("workspace_origin_unknown_bound"))?;
    if bound.phase != WorkspaceOriginPhase::Bound || bound.receipt != receipt {
        return Err(invalid("workspace_origin_binding_changed"));
    }
    let tail = &bytes[WORKSPACE_ORIGIN_FRAME_BYTES..];
    if (bound.directory || bound.control || bound.fifo) && !tail.is_empty() {
        return Err(invalid("workspace_directory_unexpected_completion"));
    }
    if tail.is_empty() {
        return Ok(bound);
    }
    if tail.len() == WORKSPACE_ORIGIN_FRAME_BYTES {
        let complete = workspace_fact(tail.try_into().unwrap())
            .ok_or_else(|| invalid("workspace_origin_unknown_completion"))?;
        let mut comparable = complete;
        comparable.phase = WorkspaceOriginPhase::Bound;
        comparable.byte_len = 0;
        comparable.payload_sha256 = [0; 32];
        if complete.phase != WorkspaceOriginPhase::Complete || comparable != bound {
            return Err(invalid("workspace_origin_operation_changed"));
        }
        let (len, hash) = actual.ok_or_else(|| invalid("workspace_origin_integrity_required"))?;
        if len != complete.byte_len || hash != complete.payload_sha256 {
            return Err(invalid("workspace_origin_payload_changed"));
        }
        return Ok(complete);
    }
    let (len, hash) = actual.ok_or_else(|| invalid("workspace_origin_unverified_partial"))?;
    let mut complete = bound;
    complete.phase = WorkspaceOriginPhase::Complete;
    complete.byte_len = len;
    complete.payload_sha256 = hash;
    if tail != &workspace_frame(complete)[..tail.len()] {
        return Err(invalid("workspace_origin_unknown_partial"));
    }
    Ok(bound)
}

#[cfg(test)]
mod workspace_origin_tests {
    use super::*;
    fn bound() -> WorkspaceOriginFacts {
        let id = FileIdentity::from_fixed_words(1, 2);
        let mut name = [0; 255];
        name[..11].copy_from_slice(b"nodes-0.csv");
        workspace_bound(
            maintenance_controls::WorkspaceOutputBinding {
                root: id,
                coordination: id,
                registration: id,
                publication_binding: id,
                registry: id,
                parent: id,
                output: id,
                incarnation: [3; 32],
                operation: [4; 32],
                name,
                name_len: 11,
            },
            id,
        )
    }
    #[test]
    fn workspace_control_binds_only_two_exact_names_and_never_complete() {
        let memory = CleanupMemory::new(32768).unwrap();
        let mut fact = bound();
        fact.control = true;
        fact.intended_name[..12].copy_from_slice(b"journal.json");
        fact.intended_name_len = 12;
        let mut frame = workspace_frame(fact);
        assert_eq!(
            decode_workspace_output_origin(&frame, fact.receipt, None, &memory).unwrap(),
            fact
        );
        let mut completed = fact;
        completed.phase = WorkspaceOriginPhase::Complete;
        assert!(workspace_fact(&workspace_frame(completed)).is_none());
        frame[504] = b'/';
        let digest = Sha256::digest(&frame[..992]);
        frame[992..].copy_from_slice(&digest);
        assert!(workspace_fact(&frame).is_none());
        let mut ordinary = workspace_frame(bound());
        ordinary[503] = 1;
        ordinary[504] = b'x';
        let digest = Sha256::digest(&ordinary[..992]);
        ordinary[992..].copy_from_slice(&digest);
        assert!(workspace_fact(&ordinary).is_none());
        let mut bytes = [0; 1025];
        bytes[..1024].copy_from_slice(&workspace_frame(fact));
        assert!(
            decode_workspace_output_origin(&bytes, fact.receipt, Some((0, [0; 32])), &memory)
                .is_err()
        );
    }
    #[test]
    fn workspace_fifo_proof_is_exact_creation_only_and_rejects_other_names() {
        let memory = CleanupMemory::new(32768).unwrap();
        let mut fact = bound();
        fact.fifo = true;
        fact.binding.name = [0; 255];
        fact.binding.name[..12].copy_from_slice(b"writers.fifo");
        fact.binding.name_len = 12;
        let frame = workspace_frame(fact);
        assert_eq!(
            decode_workspace_output_origin(&frame, fact.receipt, None, &memory).unwrap(),
            fact
        );
        for name in [b"reader.fifo".as_slice(), b"..", b"."] {
            let mut foreign = fact;
            foreign.binding.name = [0; 255];
            foreign.binding.name[..name.len()].copy_from_slice(name);
            foreign.binding.name_len = name.len() as u8;
            assert!(workspace_fact(&workspace_frame(foreign)).is_none());
        }
        let mut completed = fact;
        completed.phase = WorkspaceOriginPhase::Complete;
        assert!(workspace_fact(&workspace_frame(completed)).is_none());
        let mut tail = [0; 1025];
        tail[..1024].copy_from_slice(&frame);
        assert!(decode_workspace_output_origin(&tail, fact.receipt, None, &memory).is_err());
        let mut intended = fact;
        intended.intended_name_len = 1;
        intended.intended_name[0] = b'x';
        assert!(workspace_fact(&workspace_frame(intended)).is_none());
    }
    #[test]
    fn workspace_frames_bind_exact_creation_and_completed_bytes() {
        let memory = CleanupMemory::new(32768).unwrap();
        let bound = bound();
        let mut bytes = [0; 2048];
        bytes[..1024].copy_from_slice(&workspace_frame(bound));
        assert_eq!(
            decode_workspace_output_origin(&bytes[..1024], bound.receipt, None, &memory).unwrap(),
            bound
        );
        let mut complete = bound;
        complete.phase = WorkspaceOriginPhase::Complete;
        complete.byte_len = 7;
        complete.payload_sha256 = [8; 32];
        bytes[1024..].copy_from_slice(&workspace_frame(complete));
        assert!(decode_workspace_output_origin(&bytes, bound.receipt, None, &memory).is_err());
        assert_eq!(
            decode_workspace_output_origin(&bytes, bound.receipt, Some((7, [8; 32])), &memory)
                .unwrap(),
            complete
        );
        assert!(
            decode_workspace_output_origin(&bytes, bound.receipt, Some((8, [8; 32])), &memory)
                .is_err()
        );
        for offset in [16, 80, 176, 192, 208, 248, 1024 + 48, 2047] {
            let mut corrupt = bytes;
            corrupt[offset] ^= 1;
            assert!(decode_workspace_output_origin(
                &corrupt,
                bound.receipt,
                Some((7, [8; 32])),
                &memory
            )
            .is_err());
        }
    }
    #[test]
    fn workspace_matching_partial_completion_is_creation_only() {
        let memory = CleanupMemory::new(32768).unwrap();
        let bound = bound();
        let mut complete = bound;
        complete.phase = WorkspaceOriginPhase::Complete;
        complete.byte_len = 10;
        complete.payload_sha256 = [9; 32];
        let mut bytes = [0; 2048];
        bytes[..1024].copy_from_slice(&workspace_frame(bound));
        bytes[1024..].copy_from_slice(&workspace_frame(complete));
        for tail in [1, 96, 217, 1023] {
            assert!(decode_workspace_output_origin(
                &bytes[..1024 + tail],
                bound.receipt,
                None,
                &memory
            )
            .is_err());
            assert_eq!(
                decode_workspace_output_origin(
                    &bytes[..1024 + tail],
                    bound.receipt,
                    Some((10, [9; 32])),
                    &memory
                )
                .unwrap(),
                bound
            );
        }
    }
}
