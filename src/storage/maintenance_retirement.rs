//! Incarnation-bound Direct read/rebuild state. These control records alone do
//! not prove bundle origin, quiescence or authorize retirement of any leaf.
use super::layout::DirectLayout;
use super::locks::{
    file_identity, path_identity, FileIdentity, LockMode, RootedDirectory, WriterLease,
};
use crate::error::NativeError;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

const BLOCK: usize = 256;
const HEADER_MAGIC: &[u8; 8] = b"CGDRTH02";
const RECORD_MAGIC: &[u8; 8] = b"CGDRTS02";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectDisposition {
    Untracked,
    Active,
    Retiring,
    Retired,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Phase {
    Active = 1,
    Retiring = 2,
    Retired = 3,
}
#[derive(Clone, Copy)]
struct Header {
    root: FileIdentity,
    writer: FileIdentity,
    control: FileIdentity,
    key: [u8; 64],
    nonce: [u8; 32],
}
#[derive(Clone, Copy)]
struct Record {
    phase: Phase,
    generation: u64,
    database: FileIdentity,
    manifest: FileIdentity,
    incarnation: [u8; 32],
}
fn invalid(message: &str) -> NativeError {
    NativeError::InvalidInput(message.to_owned())
}
fn put_identity(out: &mut [u8], id: FileIdentity) {
    let (a, b) = id.fixed_words();
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..16].copy_from_slice(&b.to_le_bytes());
}
fn identity(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    )
}
fn finish(bytes: &mut [u8; BLOCK]) {
    let hash = Sha256::digest(&bytes[..BLOCK - 32]);
    bytes[BLOCK - 32..].copy_from_slice(&hash);
}
fn checked(bytes: &[u8; BLOCK], magic: &[u8; 8]) -> bool {
    &bytes[..8] == magic && bytes[BLOCK - 32..] == Sha256::digest(&bytes[..BLOCK - 32])[..]
}
fn encode_header(header: Header) -> [u8; BLOCK] {
    let mut bytes = [0; BLOCK];
    bytes[..8].copy_from_slice(HEADER_MAGIC);
    put_identity(&mut bytes[8..24], header.root);
    put_identity(&mut bytes[24..40], header.writer);
    put_identity(&mut bytes[40..56], header.control);
    bytes[56..120].copy_from_slice(&header.key);
    bytes[120..152].copy_from_slice(&header.nonce);
    finish(&mut bytes);
    bytes
}
fn decode_header(bytes: &[u8; BLOCK]) -> Option<Header> {
    if !checked(bytes, HEADER_MAGIC) || bytes[152..BLOCK - 32].iter().any(|b| *b != 0) {
        return None;
    }
    let key = bytes[56..120].try_into().ok()?;
    Some(Header {
        root: identity(&bytes[8..24]),
        writer: identity(&bytes[24..40]),
        control: identity(&bytes[40..56]),
        key,
        nonce: bytes[120..152].try_into().ok()?,
    })
}
fn encode_record(header: Header, record: Record) -> [u8; BLOCK] {
    let mut bytes = [0; BLOCK];
    bytes[..8].copy_from_slice(RECORD_MAGIC);
    bytes[8] = record.phase as u8;
    bytes[16..24].copy_from_slice(&record.generation.to_le_bytes());
    bytes[24..56].copy_from_slice(&header.nonce);
    put_identity(&mut bytes[56..72], record.database);
    put_identity(&mut bytes[72..88], record.manifest);
    bytes[88..120].copy_from_slice(&record.incarnation);
    finish(&mut bytes);
    bytes
}
fn decode_record(header: Header, bytes: &[u8; BLOCK]) -> Option<Record> {
    if !checked(bytes, RECORD_MAGIC)
        || bytes[24..56] != header.nonce
        || bytes[9..16]
            .iter()
            .chain(bytes[120..BLOCK - 32].iter())
            .any(|b| *b != 0)
    {
        return None;
    }
    let phase = match bytes[8] {
        1 => Phase::Active,
        2 => Phase::Retiring,
        3 => Phase::Retired,
        _ => return None,
    };
    Some(Record {
        phase,
        generation: u64::from_le_bytes(bytes[16..24].try_into().ok()?),
        database: identity(&bytes[56..72]),
        manifest: identity(&bytes[72..88]),
        incarnation: bytes[88..120].try_into().ok()?,
    })
}
fn component(layout: &DirectLayout) -> (Vec<u8>, [u8; 64]) {
    // This new private control uses a lossless full native pair digest. Existing
    // writer/registration/journal paths retain their established layout keys.
    let mut hash = Sha256::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        hash.update(b"direct-marker-unix-v2");
        for path in [layout.db_path(), layout.manifest_path()] {
            let bytes = path.as_os_str().as_bytes();
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        hash.update(b"direct-marker-windows-v2");
        for path in [layout.db_path(), layout.manifest_path()] {
            hash.update((path.as_os_str().encode_wide().count() as u64).to_le_bytes());
            for unit in path.as_os_str().encode_wide() {
                hash.update(unit.to_le_bytes());
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        hash.update(b"direct-marker-unsupported-v2");
        hash.update(layout.db_path().to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(layout.manifest_path().to_string_lossy().as_bytes());
    }
    let mut fixed = [0; 64];
    let digits = b"0123456789abcdef";
    for (i, byte) in hash.finalize().iter().enumerate() {
        fixed[i * 2] = digits[(byte >> 4) as usize];
        fixed[i * 2 + 1] = digits[(byte & 15) as usize];
    }
    let short = std::str::from_utf8(&fixed).expect("hex ASCII");
    (
        format!(".direct-retirement-{short}.log").into_bytes(),
        fixed,
    )
}
fn read_block(file: &mut File, offset: u64) -> std::io::Result<[u8; BLOCK]> {
    let mut bytes = [0; BLOCK];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn root_anchor(layout: &DirectLayout, write: bool) -> Result<RootedDirectory, NativeError> {
    let root = layout
        .db_path()
        .parent()
        .ok_or_else(|| invalid("direct_marker_missing_parent"))?;
    let id = path_identity(root)?;
    Ok(if write {
        RootedDirectory::open(root, id)?
    } else {
        RootedDirectory::open_readonly_root(root, id)?
    })
}
fn named_writer(
    anchor: &RootedDirectory,
    layout: &DirectLayout,
) -> Result<FileIdentity, NativeError> {
    let path = layout.writer_lock_path();
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| invalid("direct_marker_writer_name"))?;
    file_identity(&anchor.open_readonly_file(name.as_bytes())?)
}
fn bound_header(
    anchor: &RootedDirectory,
    layout: &DirectLayout,
    file: &mut File,
    key: [u8; 64],
) -> Result<Option<Header>, NativeError> {
    if file.metadata()?.len() < BLOCK as u64 {
        return Ok(None);
    }
    let Some(header) = decode_header(&read_block(file, 0)?) else {
        return Ok(None);
    };
    if header.root != anchor.identity()
        || header.writer != named_writer(anchor, layout)?
        || header.control != file_identity(file)?
        || header.key != key
    {
        return Ok(None);
    }
    Ok(Some(header))
}
fn last_record(file: &mut File, header: Header) -> Result<Option<Record>, NativeError> {
    if file.metadata()?.len() != 3 * BLOCK as u64 {
        return Ok(None);
    }
    let mut records = [None, None];
    for (index, record) in records.iter_mut().enumerate() {
        let block = read_block(file, ((index + 1) * BLOCK) as u64)?;
        if block.iter().all(|b| *b == 0) {
            continue;
        }
        let Some(decoded) = decode_record(header, &block) else {
            return Err(invalid("direct_marker_unknown_slot_preserved"));
        };
        if decoded.generation == 0 {
            return Err(invalid("direct_marker_invalid_generation"));
        }
        *record = Some(decoded);
    }
    match records {
        [None, None] => Ok(None),
        [Some(one), None] | [None, Some(one)] if one.generation == 1 => Ok(Some(one)),
        [Some(a), Some(b)] if a.generation.checked_add(1) == Some(b.generation) => Ok(Some(b)),
        [Some(a), Some(b)] if b.generation.checked_add(1) == Some(a.generation) => Ok(Some(a)),
        _ => Err(invalid("direct_marker_ambiguous_slots")),
    }
}

fn regular_identity(path: &std::path::Path) -> Result<Option<FileIdentity>, NativeError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() {
                return Ok(None);
            }
            Ok(Some(path_identity(path)?))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn observed_source_disposition(
    layout: &DirectLayout,
) -> Result<Option<DirectDisposition>, NativeError> {
    use super::maintenance_legacy_guard as legacy;
    let Some(parent) = layout.db_path().parent() else {
        return Ok(None);
    };
    if parent
        .file_name()
        .is_none_or(|name| name != ".codebaseGraph")
        || layout.manifest_path() != parent.join("manifest.json")
    {
        return Ok(None);
    }
    let root = match root_anchor(layout, false) {
        Ok(root) => root,
        Err(_) => return Ok(None),
    };
    let gate = match root.open_readonly_file(legacy::GATE) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let state = match root.open_readonly_file(legacy::STATE) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let observed = match legacy::observe_files(&root, &gate, &state) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if observed.binding.kind != legacy::LegacyScopeKind::DefaultDirect
        || layout
            .db_path()
            .file_name()
            .map(|name| name.as_encoded_bytes())
            != Some(&observed.binding.database[..observed.binding.database_len])
    {
        return Ok(None);
    }
    let mut hash = Sha256::new();
    hash.update(layout.db_path().as_os_str().as_encoded_bytes());
    hash.update([0]);
    hash.update(layout.manifest_path().as_os_str().as_encoded_bytes());
    let mut key = [0; 64];
    let digits = b"0123456789abcdef";
    for (i, byte) in hash.finalize().iter().enumerate() {
        key[i * 2] = digits[(byte >> 4) as usize];
        key[i * 2 + 1] = digits[(byte & 15) as usize];
    }
    if observed.binding.key != key {
        return Ok(None);
    }
    for (path, expected) in [
        (layout.db_path(), observed.record.database),
        (layout.manifest_path(), observed.record.manifest),
    ] {
        let Some(name) = path.file_name() else {
            return Ok(None);
        };
        match root.open_readonly_file(name.as_encoded_bytes()) {
            Ok(file) => {
                if expected.is_none_or(|stamp| stamp.recheck(&file).is_err()) {
                    return Ok(None);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Ok(None),
        }
    }
    Ok(match observed.record.phase {
        legacy::LegacyPhase::Retiring => Some(DirectDisposition::Retiring),
        legacy::LegacyPhase::Removed => Some(DirectDisposition::Retired),
        _ => None,
    })
}

/// Existing-only observation. Unknown/corrupt/missing evidence does not acquire
/// provenance, repair controls or change the primary legacy-read outcome.
pub(crate) fn observe_direct(layout: &DirectLayout) -> Result<DirectDisposition, NativeError> {
    // Actual source retirement already persists the full associated source
    // lifecycle under native/writer/root exclusion. Reading that disposition
    // grants no origin, migration or deletion authority.
    if let Some(disposition) = observed_source_disposition(layout)? {
        return Ok(disposition);
    }
    let anchor = match root_anchor(layout, false) {
        Ok(anchor) => anchor,
        Err(_) => return Ok(DirectDisposition::Unknown),
    };
    let (name, key) = component(layout);
    let mut file = match anchor.open_readonly_file(&name) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DirectDisposition::Untracked)
        }
        Err(_) => return Ok(DirectDisposition::Unknown),
    };
    let header = match bound_header(&anchor, layout, &mut file, key) {
        Ok(Some(header)) => header,
        _ => return Ok(DirectDisposition::Unknown),
    };
    let length = file.metadata()?.len();
    let Some(record) = last_record(&mut file, header).ok().flatten() else {
        return Ok(DirectDisposition::Unknown);
    };
    if file.metadata()?.len() != length {
        return Ok(DirectDisposition::Unknown);
    }
    let db = regular_identity(layout.db_path())?;
    let manifest = regular_identity(layout.manifest_path())?;
    // A new public incarnation is never addressed by an old retirement record.
    // The explicit publisher writes ACTIVE for it; an untracked replacement is
    // conservatively Unknown and still receives the ordinary primary read.
    if db.is_some_and(|id| id != record.database)
        || manifest.is_some_and(|id| id != record.manifest)
    {
        return Ok(DirectDisposition::Unknown);
    }
    Ok(match record.phase {
        Phase::Active if db == Some(record.database) && manifest == Some(record.manifest) => {
            DirectDisposition::Active
        }
        Phase::Active => DirectDisposition::Unknown,
        Phase::Retiring => DirectDisposition::Retiring,
        Phase::Retired => DirectDisposition::Retired,
    })
}
/// Called only after a normal explicit Direct publication while retaining the
/// ACTUAL exclusive writer. The new pair is ACTIVE even after an older pair's
/// retirement. Unknown control collisions are preserved, never overwritten.
pub(crate) fn mark_explicit_direct_rebuild_active(
    layout: &DirectLayout,
    writer: &WriterLease,
) -> Result<(), NativeError> {
    if !writer.verifies(&layout.writer_lock_path(), LockMode::Exclusive)? {
        return Err(invalid("direct_active_requires_actual_writer"));
    }
    let Some(database) = regular_identity(layout.db_path())? else {
        return Err(invalid("direct_active_database_missing"));
    };
    let Some(manifest) = regular_identity(layout.manifest_path())? else {
        return Err(invalid("direct_active_manifest_missing"));
    };
    let anchor = root_anchor(layout, true)?;
    if named_writer(&anchor, layout)? != writer.identity()? {
        return Err(invalid("direct_active_writer_changed"));
    }
    let (name, key) = component(layout);
    let mut fresh = false;
    let mut file = match anchor.open_existing(&name) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let file = anchor.create_new(&name)?;
            fresh = true;
            file
        }
        Err(e) => return Err(e.into()),
    };
    let header = if fresh {
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| invalid("direct_marker_entropy_unavailable"))?;
        let header = Header {
            root: anchor.identity(),
            writer: writer.identity()?,
            control: file_identity(&file)?,
            key,
            nonce,
        };
        super::locks::set_created_file_nonce(&file, &header.nonce)?;
        file.write_all(&encode_header(header))?;
        file.write_all(&[0; 2 * BLOCK])?;
        file.sync_all()?;
        anchor.sync_namespace()?;
        header
    } else {
        let Some(header) = bound_header(&anchor, layout, &mut file, key)? else {
            return Err(invalid("direct_marker_unknown_preserved"));
        };
        header
    };
    let mut incarnation = [0; 32];
    getrandom::fill(&mut incarnation).map_err(|_| invalid("direct_active_entropy_unavailable"))?;
    append_record(
        &mut file,
        header,
        Record {
            phase: Phase::Active,
            generation: 0,
            database,
            manifest,
            incarnation,
        },
    )?;
    if !writer.verifies(&layout.writer_lock_path(), LockMode::Exclusive)?
        || regular_identity(layout.db_path())? != Some(database)
        || regular_identity(layout.manifest_path())? != Some(manifest)
    {
        return Err(invalid("direct_active_publication_changed"));
    }
    Ok(())
}
fn append_record(file: &mut File, header: Header, mut record: Record) -> Result<(), NativeError> {
    if file.metadata()?.len() != 3 * BLOCK as u64 {
        return Err(invalid("direct_marker_size_unknown_preserved"));
    }
    let old = last_record(file, header)?;
    record.generation = old
        .map_or(Some(1), |record| record.generation.checked_add(1))
        .ok_or_else(|| invalid("direct_marker_generation_exhausted"))?;
    let slot = ((record.generation - 1) % 2) as usize;
    let bytes = encode_record(header, record);
    let offset = ((slot + 1) * BLOCK) as u64;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if read_block(file, offset)? != bytes {
        return Err(invalid("direct_marker_readback_changed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_marker_fixed_slots_preserve_full_native_pair_identity() {
        let a = DirectLayout::new("/a/db.ldb", "/a/manifest.json");
        let b = DirectLayout::new("/a/db.ldb", "/b/manifest.json");
        let (an, ak) = component(&a);
        let (bn, bk) = component(&b);
        assert_ne!(an, bn);
        assert_ne!(ak, bk);
        assert!(an.len() <= 255);
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let p =
                std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/a/db\xff.ldb".to_vec()));
            let q =
                std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/a/db\xfe.ldb".to_vec()));
            assert_ne!(
                component(&DirectLayout::new(p, "/a/manifest.json")).1,
                component(&DirectLayout::new(q, "/a/manifest.json")).1
            );
        }
    }
    #[test]
    fn direct_marker_record_generation_and_corruption_are_checked() {
        let id = FileIdentity::from_fixed_words(1, 2);
        let header = Header {
            root: id,
            writer: id,
            control: id,
            key: [b'a'; 64],
            nonce: [3; 32],
        };
        let record = Record {
            phase: Phase::Active,
            generation: 7,
            database: id,
            manifest: id,
            incarnation: [4; 32],
        };
        let bytes = encode_record(header, record);
        let read = decode_record(header, &bytes).unwrap();
        assert_eq!(read.generation, 7);
        assert_eq!(read.database, id);
        let mut bad = bytes;
        bad[16] ^= 1;
        assert!(decode_record(header, &bad).is_none());
    }
}
