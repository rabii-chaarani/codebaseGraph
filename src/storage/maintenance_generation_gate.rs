//! Generation lifecycle controls shared by native startup and managed reads.
//! This module imports no API/native-verified token or legacy association job.
use super::layout::{GenerationPaths, ManagedLayout};
use super::locks::{
    self, FileIdentity, FixedControlKind, FixedFileStamp, FixedLockedFile, LockMode,
    RootedDirectory,
};
use super::maintenance_budget::{CleanupMemory, MemoryBox, MutationGrant};
use super::maintenance_controls as controls;
use crate::error::NativeError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
pub(crate) const ORIGIN_BYTES: usize = 2048;
pub(crate) const FINAL_BYTES: usize = 512;
pub(crate) const ORIGIN_NAME: &[u8] = b".generation-origin";
pub(crate) const FINAL_NAME: &[u8] = b".generation-final";
pub(crate) const PAYLOADS: usize = 12;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GenerationOriginScope {
    pub(crate) root: FileIdentity,
    pub(crate) coordination: FileIdentity,
    pub(crate) cohort: FileIdentity,
    pub(crate) publication_binding: FileIdentity,
    pub(crate) incarnation: [u8; 32],
}
pub(crate) fn invalid(reason: &str) -> NativeError {
    NativeError::InvalidInput(reason.into())
}

pub(crate) fn stamp(file: &File) -> Result<FixedFileStamp, NativeError> {
    FixedFileStamp::capture(file, FixedControlKind::File)
        .map_err(|_| invalid("generation_source_changed_or_aliased"))
}
pub(crate) fn recheck(before: FixedFileStamp, file: &File) -> Result<(), NativeError> {
    before
        .recheck(file)
        .map_err(|_| invalid("generation_source_changed"))
}
pub(crate) fn id_put(b: &mut [u8], id: FileIdentity) {
    let (a, c) = id.fixed_words();
    b[..8].copy_from_slice(&a.to_le_bytes());
    b[8..16].copy_from_slice(&c.to_le_bytes());
}
pub(crate) fn id_get(b: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(b[..8].try_into().unwrap()),
        u64::from_le_bytes(b[8..16].try_into().unwrap()),
    )
}
pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub(crate) fn component(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_'))
}
pub(crate) fn fixed_id(text: &str) -> Result<([u8; 128], u8), NativeError> {
    if !component(text.as_bytes()) {
        return Err(invalid("invalid_generation_id"));
    }
    let mut out = [0; 128];
    out[..text.len()].copy_from_slice(text.as_bytes());
    Ok((out, text.len() as u8))
}
pub(crate) fn gen_name(id: &[u8; 128], len: u8) -> Result<([u8; 132], usize), NativeError> {
    let len = usize::from(len);
    if len == 0 || len > id.len() {
        return Err(invalid("invalid_generation_component"));
    }
    if !component(&id[..len]) || id[len..].iter().any(|b| *b != 0) {
        return Err(invalid("invalid_generation_component"));
    }
    let mut name = [0; 132];
    name[..4].copy_from_slice(b"gen-");
    name[4..4 + len].copy_from_slice(&id[..len]);
    Ok((name, len + 4))
}
pub(crate) fn resource_name() -> [u8; 83] {
    *b".database-use-97ad9f2c7f86b10f58a6f136bf8579cf8d878c8c33a2dd9acbc5217e639a11c6.lock"
}
pub(crate) fn resource_name_for(basename: &[u8]) -> [u8; 83] {
    let hash = digest(basename);
    let mut name = [0; 83];
    name[..14].copy_from_slice(b".database-use-");
    for (i, b) in hash.iter().enumerate() {
        name[14 + i * 2] = b"0123456789abcdef"[(b >> 4) as usize];
        name[15 + i * 2] = b"0123456789abcdef"[(b & 15) as usize];
    }
    name[78..].copy_from_slice(b".lock");
    name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenerationDisposition {
    Untracked,
    Published,
    Final,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoundLeaf {
    pub(crate) id: FileIdentity,
    pub(crate) len: u64,
    pub(crate) hash: [u8; 32],
    pub(crate) stamp_digest: [u8; 32],
}
#[derive(Clone, Copy)]
pub(crate) struct Origin {
    pub(crate) generation: [u8; 128],
    pub(crate) generation_len: u8,
    pub(crate) scope: GenerationOriginScope,
    pub(crate) managed: FileIdentity,
    pub(crate) writer: FileIdentity,
    pub(crate) directory: FileIdentity,
    pub(crate) receipt: FileIdentity,
    pub(crate) operation: [u8; 32],
    pub(crate) lease: FileIdentity,
    pub(crate) family: FileIdentity,
    pub(crate) resource: FileIdentity,
    pub(crate) manifest_resource: FileIdentity,
    pub(crate) native_lock: Option<FileIdentity>,
    pub(crate) leaves: [Option<BoundLeaf>; PAYLOADS],
}
impl Origin {
    pub(crate) fn encode(self) -> [u8; ORIGIN_BYTES] {
        let mut b = [0; ORIGIN_BYTES];
        b[..8].copy_from_slice(b"CGGEN001");
        b[8..16].copy_from_slice(&2u64.to_le_bytes());
        b[16] = self.generation_len;
        b[32..160].copy_from_slice(&self.generation);
        for (o, id) in [
            (160, self.scope.root),
            (176, self.scope.coordination),
            (192, self.scope.cohort),
            (208, self.scope.publication_binding),
            (256, self.managed),
            (272, self.writer),
            (288, self.directory),
            (304, self.receipt),
            (352, self.lease),
            (368, self.family),
            (384, self.resource),
        ] {
            id_put(&mut b[o..o + 16], id);
        }
        b[224..256].copy_from_slice(&self.scope.incarnation);
        b[320..352].copy_from_slice(&self.operation);
        for (i, leaf) in self.leaves.iter().enumerate() {
            if let Some(leaf) = leaf {
                let at = 400 + i * 112;
                b[at] = 1;
                id_put(&mut b[at + 8..at + 24], leaf.id);
                b[at + 24..at + 32].copy_from_slice(&leaf.len.to_le_bytes());
                b[at + 32..at + 64].copy_from_slice(&leaf.hash);
                b[at + 64..at + 96].copy_from_slice(&leaf.stamp_digest);
            }
        }
        id_put(&mut b[1744..1760], self.manifest_resource);
        if let Some(id) = self.native_lock {
            b[1760] = 1;
            id_put(&mut b[1768..1784], id);
        }
        let hash = digest(&b[..2016]);
        b[2016..].copy_from_slice(&hash);
        b
    }
    pub(crate) fn decode(b: &[u8; ORIGIN_BYTES]) -> Option<Self> {
        if &b[..8] != b"CGGEN001"
            || u64::from_le_bytes(b[8..16].try_into().ok()?) != 2
            || b[1760] > 1
            || b[1761..1768].iter().any(|v| *v != 0)
            || (b[1760] == 0 && b[1768..1784].iter().any(|v| *v != 0))
            || b[17..32]
                .iter()
                .chain(b[1784..2016].iter())
                .any(|v| *v != 0)
            || digest(&b[..2016]) != b[2016..]
        {
            return None;
        }
        if b[224..256].iter().all(|v| *v == 0) || b[320..352].iter().all(|v| *v == 0) {
            return None;
        }
        let generation = b[32..160].try_into().ok()?;
        let generation_len = b[16];
        gen_name(&generation, generation_len).ok()?;
        let mut leaves = [None; PAYLOADS];
        for (i, leaf) in leaves.iter_mut().enumerate() {
            let at = 400 + i * 112;
            if b[at] > 1
                || b[at + 1..at + 8]
                    .iter()
                    .chain(b[at + 96..at + 112].iter())
                    .any(|v| *v != 0)
            {
                return None;
            }
            if b[at] == 0 {
                if b[at..at + 112].iter().any(|v| *v != 0) {
                    return None;
                }
            } else {
                *leaf = Some(BoundLeaf {
                    id: id_get(&b[at + 8..at + 24]),
                    len: u64::from_le_bytes(b[at + 24..at + 32].try_into().ok()?),
                    hash: b[at + 32..at + 64].try_into().ok()?,
                    stamp_digest: b[at + 64..at + 96].try_into().ok()?,
                });
            }
        }
        if leaves[..4].iter().any(Option::is_none) {
            return None;
        }
        let search = leaves[6..].iter().filter(|v| v.is_some()).count();
        if search != 0 && search != 6 {
            return None;
        }
        Some(Self {
            generation,
            generation_len,
            scope: GenerationOriginScope {
                root: id_get(&b[160..176]),
                coordination: id_get(&b[176..192]),
                cohort: id_get(&b[192..208]),
                publication_binding: id_get(&b[208..224]),
                incarnation: b[224..256].try_into().ok()?,
            },
            managed: id_get(&b[256..272]),
            writer: id_get(&b[272..288]),
            directory: id_get(&b[288..304]),
            receipt: id_get(&b[304..320]),
            operation: b[320..352].try_into().ok()?,
            lease: id_get(&b[352..368]),
            family: id_get(&b[368..384]),
            resource: id_get(&b[384..400]),
            manifest_resource: id_get(&b[1744..1760]),
            native_lock: (b[1760] == 1).then(|| id_get(&b[1768..1784])),
            leaves,
        })
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Final {
    pub(crate) generation: [u8; 128],
    pub(crate) generation_len: u8,
    pub(crate) managed: FileIdentity,
    pub(crate) directory: FileIdentity,
    pub(crate) origin: FileIdentity,
    pub(crate) origin_digest: [u8; 32],
    pub(crate) receipt: FileIdentity,
    pub(crate) successor: FileIdentity,
    pub(crate) successor_origin: FileIdentity,
    pub(crate) successor_id: [u8; 128],
    pub(crate) successor_len: u8,
    pub(crate) successor_digest: [u8; 32],
}
impl Final {
    pub(crate) fn encode(self) -> [u8; FINAL_BYTES] {
        let mut b = [0; FINAL_BYTES];
        b[..8].copy_from_slice(b"CGFINAL1");
        b[8..16].copy_from_slice(&1u64.to_le_bytes());
        b[16] = self.generation_len;
        b[32..160].copy_from_slice(&self.generation);
        for (o, id) in [
            (160, self.managed),
            (176, self.directory),
            (192, self.origin),
            (240, self.receipt),
            (256, self.successor),
            (272, self.successor_origin),
        ] {
            id_put(&mut b[o..o + 16], id);
        }
        b[208..240].copy_from_slice(&self.origin_digest);
        b[288] = self.successor_len;
        b[296..424].copy_from_slice(&self.successor_id);
        b[424..456].copy_from_slice(&self.successor_digest);
        let hash = digest(&b[..480]);
        b[480..].copy_from_slice(&hash);
        b
    }
    pub(crate) fn decode(b: &[u8; FINAL_BYTES]) -> Option<Self> {
        if &b[..8] != b"CGFINAL1"
            || u64::from_le_bytes(b[8..16].try_into().ok()?) != 1
            || b[17..32]
                .iter()
                .chain(b[289..296].iter())
                .chain(b[456..480].iter())
                .any(|v| *v != 0)
            || digest(&b[..480]) != b[480..]
        {
            return None;
        }
        let generation = b[32..160].try_into().ok()?;
        gen_name(&generation, b[16]).ok()?;
        let successor_id = b[296..424].try_into().ok()?;
        gen_name(&successor_id, b[288]).ok()?;
        if generation == successor_id {
            return None;
        }
        Some(Self {
            generation,
            generation_len: b[16],
            managed: id_get(&b[160..176]),
            directory: id_get(&b[176..192]),
            origin: id_get(&b[192..208]),
            origin_digest: b[208..240].try_into().ok()?,
            receipt: id_get(&b[240..256]),
            successor: id_get(&b[256..272]),
            successor_origin: id_get(&b[272..288]),
            successor_id,
            successor_len: b[288],
            successor_digest: b[424..456].try_into().ok()?,
        })
    }
}
pub(crate) fn exact<const N: usize>(file: &File) -> Result<[u8; N], NativeError> {
    let before = stamp(file)?;
    if before.len() != N as u64 {
        return Err(invalid("generation_record_size"));
    }
    let mut cursor = file;
    cursor.seek(SeekFrom::Start(0))?;
    let mut bytes = [0; N];
    cursor.read_exact(&mut bytes)?;
    recheck(before, file)?;
    Ok(bytes)
}
pub(crate) fn open_generation(
    root: &RootedDirectory,
    id: &[u8; 128],
    len: u8,
) -> Result<RootedDirectory, NativeError> {
    let (name, n) = gen_name(id, len)?;
    Ok(root
        .open_directory_readonly(b"generations")?
        .open_directory_readonly(&name[..n])?)
}
pub(crate) fn read_origin(
    directory: &RootedDirectory,
    managed: FileIdentity,
) -> Result<(Origin, [u8; ORIGIN_BYTES], FixedFileStamp), NativeError> {
    let file = directory.open_readonly_file(ORIGIN_NAME)?;
    let before = stamp(&file)?;
    let bytes = exact(&file)?;
    let origin = Origin::decode(&bytes).ok_or_else(|| invalid("generation_origin_unknown"))?;
    if origin.directory != directory.identity()
        || origin.managed != managed
        || origin.receipt != before.identity()
    {
        return Err(invalid("generation_origin_incarnation_changed"));
    }
    Ok((origin, bytes, before))
}
pub(crate) fn read_final(
    directory: &RootedDirectory,
    origin: Origin,
    origin_bytes: &[u8; ORIGIN_BYTES],
) -> Result<(Final, [u8; FINAL_BYTES], FixedFileStamp), NativeError> {
    let file = directory.open_readonly_file(FINAL_NAME)?;
    let before = stamp(&file)?;
    let bytes = exact(&file)?;
    let final_record = Final::decode(&bytes).ok_or_else(|| invalid("generation_final_unknown"))?;
    if final_record.directory != origin.directory
        || final_record.managed != origin.managed
        || final_record.generation != origin.generation
        || final_record.generation_len != origin.generation_len
        || final_record.origin != origin.receipt
        || final_record.origin_digest != digest(origin_bytes)
        || final_record.receipt != before.identity()
    {
        return Err(invalid("generation_final_incarnation_changed"));
    }
    Ok((final_record, bytes, before))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Current {
    schema_version: u64,
    generation_id: Text,
    published_at: Text,
    activated_at_ms: u64,
}
struct Text {
    bytes: [u8; 128],
    len: u8,
}
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Text;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bounded generation control string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Text, E> {
                if v.len() > 128 {
                    return Err(E::custom("generation control string too long"));
                }
                let mut bytes = [0; 128];
                bytes[..v.len()].copy_from_slice(v.as_bytes());
                Ok(Text {
                    bytes,
                    len: v.len() as u8,
                })
            }
        }
        de.deserialize_str(V)
    }
}
pub(crate) fn read_current(
    root: &RootedDirectory,
) -> Result<([u8; 128], u8, FixedFileStamp), NativeError> {
    let file = root.open_readonly_file(b"active.json")?;
    let before = stamp(&file)?;
    if before.len() > 1024 {
        return Err(invalid("generation_current_size"));
    }
    let mut bytes = [0; 1024];
    let mut cursor = &file;
    cursor.read_exact(&mut bytes[..before.len() as usize])?;
    let current: Current = serde_json::from_slice(&bytes[..before.len() as usize])?;
    let _ = (current.published_at, current.activated_at_ms);
    if current.schema_version != 2
        || !component(&current.generation_id.bytes[..usize::from(current.generation_id.len)])
    {
        return Err(invalid("generation_current_invalid"));
    }
    recheck(before, &file)?;
    Ok((
        current.generation_id.bytes,
        current.generation_id.len,
        before,
    ))
}

pub(crate) fn observe_generation_disposition(
    layout: &ManagedLayout,
    paths: &GenerationPaths,
    memory: &CleanupMemory,
) -> Result<GenerationDisposition, NativeError> {
    let _floor = memory
        .reserve(ORIGIN_BYTES * 3 + FINAL_BYTES * 3 + 8192 + std::mem::size_of::<Origin>() * 2)
        .map_err(|_| invalid("generation_memory_limit"))?;
    let root = RootedDirectory::open_readonly_root(
        layout.storage_root(),
        locks::path_identity(layout.storage_root())?,
    )?;
    let (id, len) = fixed_id(paths.generation_id())?;
    let directory = open_generation(&root, &id, len)?;
    let (origin, bytes, _) = match read_origin(&directory, root.identity()) {
        Ok(value) => value,
        Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(match read_legacy_final(&directory, root.identity()) {
                Ok((closed, _, _))
                    if closed.generation == id
                        && closed.generation_len == len
                        && current_parent_scope(&root, memory)
                            .is_ok_and(|scope| scope == closed.scope) =>
                {
                    GenerationDisposition::Final
                }
                Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    GenerationDisposition::Untracked
                }
                _ => GenerationDisposition::Unknown,
            })
        }
        Err(_) => return Ok(GenerationDisposition::Unknown),
    };
    if origin.generation != id
        || origin.generation_len != len
        || !current_parent_scope(&root, memory).is_ok_and(|scope| scope == origin.scope)
    {
        return Ok(GenerationDisposition::Unknown);
    }
    match read_final(&directory, origin, &bytes) {
        Ok(_) => Ok(GenerationDisposition::Final),
        Err(NativeError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(GenerationDisposition::Published)
        }
        Err(_) => Ok(GenerationDisposition::Unknown),
    }
}

const START_GATE: &[u8] = b".generation-startup.lock";
const START_BINDING: &[u8] = b".generation-startup.binding";
const START_DATA: [&[u8]; 2] = [b".generation-startup.data0", b".generation-startup.data1"];
const START_SELECTOR: &[u8] = b".generation-startup.selector";
#[derive(Clone, Copy)]
pub(super) struct GateBinding {
    pub(super) managed: FileIdentity,
    pub(super) generations: FileIdentity,
    pub(super) writer: FileIdentity,
    pub(super) gate: FileIdentity,
    pub(super) binding: FileIdentity,
    pub(super) data: [FileIdentity; 2],
    pub(super) selector: FileIdentity,
    pub(super) parent: GenerationOriginScope,
    pub(super) nonce: [u8; 32],
}
impl GateBinding {
    pub(super) fn encode(self) -> [u8; 512] {
        let mut b = [0; 512];
        b[..8].copy_from_slice(b"CGGGATE1");
        b[8..16].copy_from_slice(&1u64.to_le_bytes());
        for (o, id) in [
            (16, self.managed),
            (32, self.gate),
            (48, self.data[0]),
            (64, self.data[1]),
            (80, self.selector),
            (96, self.binding),
            (112, self.parent.root),
            (128, self.parent.coordination),
            (144, self.parent.cohort),
            (160, self.parent.publication_binding),
            (240, self.generations),
            (256, self.writer),
        ] {
            id_put(&mut b[o..o + 16], id);
        }
        b[176..208].copy_from_slice(&self.parent.incarnation);
        b[208..240].copy_from_slice(&self.nonce);
        let hash = digest(&b[..480]);
        b[480..].copy_from_slice(&hash);
        b
    }
    fn decode(b: &[u8; 512]) -> Option<Self> {
        if &b[..8] != b"CGGGATE1"
            || u64::from_le_bytes(b[8..16].try_into().ok()?) != 1
            || b[272..480].iter().any(|v| *v != 0)
            || digest(&b[..480]) != b[480..]
            || b[208..240].iter().all(|v| *v == 0)
        {
            return None;
        }
        Some(Self {
            managed: id_get(&b[16..32]),
            gate: id_get(&b[32..48]),
            data: [id_get(&b[48..64]), id_get(&b[64..80])],
            selector: id_get(&b[80..96]),
            binding: id_get(&b[96..112]),
            parent: GenerationOriginScope {
                root: id_get(&b[112..128]),
                coordination: id_get(&b[128..144]),
                cohort: id_get(&b[144..160]),
                publication_binding: id_get(&b[160..176]),
                incarnation: b[176..208].try_into().ok()?,
            },
            nonce: b[208..240].try_into().ok()?,
            generations: id_get(&b[240..256]),
            writer: id_get(&b[256..272]),
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
// This Copy control image has a fixed accounted layout; boxing would add allocation and change the verified admission handoff.
#[allow(clippy::large_enum_variant)]
enum GateState {
    Idle,
    Removing {
        generation: [u8; 128],
        len: u8,
        directory: FileIdentity,
        origin: FileIdentity,
        final_record: FileIdentity,
        proof_digest: [u8; 32],
    },
}
impl GateState {
    fn encode(self) -> [u8; 256] {
        let mut b = [0; 256];
        b[..8].copy_from_slice(b"CGGRMV01");
        if let Self::Removing {
            generation,
            len,
            directory,
            origin,
            final_record,
            proof_digest,
        } = self
        {
            b[8] = 1;
            b[9] = len;
            b[16..144].copy_from_slice(&generation);
            id_put(&mut b[144..160], directory);
            id_put(&mut b[160..176], origin);
            id_put(&mut b[176..192], final_record);
            b[192..224].copy_from_slice(&proof_digest);
        }
        let hash = digest(&b[..224]);
        b[224..].copy_from_slice(&hash);
        b
    }
    fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != 256
            || &b[..8] != b"CGGRMV01"
            || b[10..16].iter().any(|v| *v != 0)
            || digest(&b[..224]) != b[224..]
        {
            return None;
        }
        match b[8] {
            0 if b[9..224].iter().all(|v| *v == 0) => Some(Self::Idle),
            1 => {
                let generation = b[16..144].try_into().ok()?;
                gen_name(&generation, b[9]).ok()?;
                Some(Self::Removing {
                    generation,
                    len: b[9],
                    directory: id_get(&b[144..160]),
                    origin: id_get(&b[160..176]),
                    final_record: id_get(&b[176..192]),
                    proof_digest: b[192..224].try_into().ok()?,
                })
            }
            _ => None,
        }
    }
}
struct FreshGateFiles<'a> {
    root: &'a RootedDirectory,
    files: [Option<(&'static [u8], FileIdentity)>; 5],
    active: bool,
}
impl FreshGateFiles<'_> {
    fn create(&mut self, name: &'static [u8], bytes: &[u8]) -> Result<FileIdentity, NativeError> {
        let mut file = self.root.create_new(name)?;
        let id = locks::file_identity(&file)?;
        let slot = self
            .files
            .iter_mut()
            .find(|v| v.is_none())
            .ok_or_else(|| invalid("generation_gate_bootstrap_capacity"))?;
        *slot = Some((name, id));
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(id)
    }
}
impl Drop for FreshGateFiles<'_> {
    fn drop(&mut self) {
        if !self.active {
            for (name, id) in self.files.iter().rev().flatten() {
                let _ = self.root.unlink_file(name, *id);
            }
            let _ = self.root.sync_namespace();
        }
    }
}
pub(super) fn legacy_bootstrap_idle_image() -> [u8; 256] {
    GateState::Idle.encode()
}

/// Sole genuine fresh-reference-root factory. No ordinary acquisition, old
/// directory, serialized IDs or read-only observation can enroll this protocol.
pub(crate) fn initialize_fresh_generation_gate(
    fresh: &super::layout::FreshReferenceRoot,
    writer: &locks::WriterLease,
    creation: &super::maintenance_ownership::RootCreationAuthority<'_>,
) -> Result<(), NativeError> {
    let layout = ManagedLayout::new(fresh.path());
    if !writer.verifies(&layout.writer_lock_path(), LockMode::Exclusive)?
        || locks::path_identity(fresh.path())? != fresh.identity()
    {
        return Err(invalid("generation_gate_fresh_writer_unverified"));
    }
    let root = RootedDirectory::open(fresh.path(), fresh.identity())?;
    let generations = root.open_directory_readonly(b"generations")?;
    let mut names = [0; 4096];
    let mut empty = true;
    generations.visit_entries(&mut names, &mut |_| {
        empty = false;
        Ok(())
    })?;
    if !empty {
        return Err(invalid("generation_gate_fresh_namespace_not_empty"));
    }
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| invalid("generation_gate_entropy_unavailable"))?;
    let mut created = FreshGateFiles {
        root: &root,
        files: [None; 5],
        active: false,
    };
    let gate = created.create(START_GATE, &locks::encode_coordination_incarnation(nonce))?;
    let mut slot = [0; controls::SLOT_BYTES];
    let first = controls::encode_slot(&mut slot, &nonce, 0, &GateState::Idle.encode())
        .map_err(|_| invalid("generation_gate_codec"))?;
    let data0 = created.create(START_DATA[0], &slot)?;
    controls::encode_slot(&mut slot, &nonce, 0, b"")
        .map_err(|_| invalid("generation_gate_codec"))?;
    let data1 = created.create(START_DATA[1], &slot)?;
    let mut selector = [0; controls::SELECTOR_BYTES];
    controls::encode_commit(
        &mut selector[..controls::COMMIT_BYTES],
        &nonce,
        controls::CommitFact {
            generation: 0,
            active: 0,
            slot_digest: first.digest,
        },
    )
    .map_err(|_| invalid("generation_gate_codec"))?;
    let selector_id = created.create(START_SELECTOR, &selector)?;
    let binding_id = created.create(START_BINDING, b"")?;
    let mut binding_file = root.open_existing(START_BINDING)?;
    let binding = GateBinding {
        managed: root.identity(),
        generations: generations.identity(),
        writer: writer.identity()?,
        gate,
        binding: binding_id,
        data: [data0, data1],
        selector: selector_id,
        parent: GenerationOriginScope {
            root: creation.root_identity(),
            coordination: creation.coordination_identity()?,
            cohort: creation.cohort_identity()?,
            publication_binding: creation.publication_binding_identity()?,
            incarnation: creation.incarnation()?,
        },
        nonce,
    };
    // Persist every actual namespace/inode BEFORE publishing activation bytes.
    // Unsupported directory durability leaves an unactivated finite bootstrap;
    // ordinary native use may stay untracked, while root removal stays disabled.
    root.sync_namespace()?;
    binding_file.write_all(&binding.encode())?;
    binding_file.sync_all()?;
    created.active = true;
    Ok(())
}
fn current_parent_scope(
    root: &RootedDirectory,
    memory: &CleanupMemory,
) -> Result<GenerationOriginScope, NativeError> {
    let artifacts = root.open_directory_readonly(b"artifacts")?;
    let scope = super::maintenance_origin::observe_artifact_root_scope(&artifacts, memory)?
        .ok_or_else(|| invalid("generation_gate_parent_unverified"))?;
    Ok(GenerationOriginScope {
        root: scope.root,
        coordination: scope.coordination,
        cohort: scope.cohort,
        publication_binding: scope.publication_binding,
        incarnation: scope.incarnation,
    })
}
fn load_gate(
    root: &RootedDirectory,
    memory: &CleanupMemory,
    bootstrap_path: Option<&Path>,
) -> Result<Option<GateBinding>, NativeError> {
    let staged = || -> Result<bool, NativeError> {
        let Some(path) = bootstrap_path else {
            return Ok(false);
        };
        let domain =
            super::maintenance_ownership::ReferenceDomain::managed(&ManagedLayout::new(path))?;
        domain
            .staged_generation_gate(root)
            .map_err(|_| invalid("generation_gate_bootstrap_unverified"))
    };
    let file = match root.open_readonly_file(START_BINDING) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match root.open_readonly_file(START_GATE) {
                Ok(_) if staged()? => return Ok(None),
                Ok(_) => return Err(invalid("generation_gate_incomplete")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    for name in [START_DATA[0], START_DATA[1], START_SELECTOR] {
                        match root.open_readonly_file(name) {
                            Ok(_) => return Err(invalid("generation_gate_incomplete")),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(e) => return Err(e.into()),
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let image = exact(&file);
    let binding = match image.as_ref().ok().and_then(GateBinding::decode) {
        Some(binding) => binding,
        None if staged()? => return Ok(None),
        None => return Err(invalid("generation_gate_binding_unknown")),
    };
    if root.identity() != binding.managed
        || locks::file_identity(&file)? != binding.binding
        || root.open_directory_readonly(b"generations")?.identity() != binding.generations
        || current_parent_scope(root, memory)? != binding.parent
    {
        return Err(invalid("generation_gate_binding_changed"));
    }
    Ok(Some(binding))
}
fn read_gate_state(
    root: &RootedDirectory,
    binding: GateBinding,
) -> Result<(controls::CommitFact, GateState), NativeError> {
    let selector = root.open_readonly_file(START_SELECTOR)?;
    if locks::file_identity(&selector)? != binding.selector {
        return Err(invalid("generation_gate_selector_changed"));
    }
    let image = exact::<{ controls::SELECTOR_BYTES }>(&selector)?;
    let mut slots = [None; 2];
    let mut payloads = [[0; controls::SLOT_BYTES]; 2];
    for i in 0..2 {
        let file = root.open_readonly_file(START_DATA[i])?;
        if locks::file_identity(&file)? != binding.data[i] {
            return Err(invalid("generation_gate_slot_changed"));
        }
        payloads[i] = exact(&file)?;
        slots[i] = controls::decode_slot(&payloads[i], &binding.nonce).ok();
    }
    let commit = controls::decode_commit(&image[..controls::COMMIT_BYTES], &binding.nonce).ok();
    let intent = controls::decode_intent(&image[controls::COMMIT_BYTES..], &binding.nonce).ok();
    let commit = match controls::recovery_step(commit, intent, slots) {
        controls::RecoveryStep::Committed(commit) => commit,
        controls::RecoveryStep::RewriteIntendedSlot(i)
        | controls::RecoveryStep::CommitIntendedSlot(i) => {
            // A valid complete Intent names the closed directory even when its
            // target slot/commit still needs recovery. Other generations remain
            // usable; no startup mutates or guesses the highest slot.
            let intended = &image
                [controls::COMMIT_BYTES + 96..controls::COMMIT_BYTES + 96 + controls::SLOT_BYTES];
            let fact = controls::decode_slot(intended, &binding.nonce)
                .map_err(|_| invalid("generation_gate_unknown_intent"))?;
            let state = GateState::decode(&intended[56..56 + fact.payload_len])
                .ok_or_else(|| invalid("generation_gate_unknown_state"))?;
            return Ok((
                controls::CommitFact {
                    generation: i.after,
                    active: i.target,
                    slot_digest: i.slot_digest,
                },
                state,
            ));
        }
        controls::RecoveryStep::Unknown => return Err(invalid("generation_gate_recovery_unknown")),
    };
    let fact = slots[commit.active].ok_or_else(|| invalid("generation_gate_unknown_slot"))?;
    let state = GateState::decode(&payloads[commit.active][56..56 + fact.payload_len])
        .ok_or_else(|| invalid("generation_gate_unknown_state"))?;
    Ok((commit, state))
}
pub(crate) struct GenerationStartupGuard {
    root: RootedDirectory,
    gate: FixedLockedFile,
    binding: GateBinding,
    parent: FileIdentity,
    parent_path: PathBuf,
}
fn locate_startup(
    database: &Path,
) -> Result<Option<(RootedDirectory, GateBinding, PathBuf, FileIdentity)>, NativeError> {
    let absolute = if database.is_absolute() {
        database.to_path_buf()
    } else {
        std::env::current_dir()?.join(database)
    };
    let parent = std::fs::canonicalize(
        absolute
            .parent()
            .ok_or_else(|| invalid("generation_parent_missing"))?,
    )?;
    let Some(generations_path) = parent.parent() else {
        return Ok(None);
    };
    let Some(managed_path) = generations_path.parent() else {
        return Ok(None);
    };
    let root =
        RootedDirectory::open_readonly_root(managed_path, locks::path_identity(managed_path)?)?;
    let memory = CleanupMemory::new(64 * 1024).map_err(|_| invalid("generation_startup_memory"))?;
    let Some(binding) = load_gate(&root, &memory, Some(managed_path))? else {
        let parent_id = locks::path_identity(&parent)?;
        let directory = RootedDirectory::open_readonly_root(&parent, parent_id)?;
        if let Ok((origin, image, _)) = read_origin(&directory, root.identity()) {
            if current_parent_scope(&root, &memory).is_ok_and(|scope| scope == origin.scope)
                && read_final(&directory, origin, &image).is_ok()
            {
                return Err(invalid("managed_generation_retired_rebuild_required"));
            }
        }
        if let Ok((closed, _, _)) = read_legacy_final(&directory, root.identity()) {
            if current_parent_scope(&root, &memory).is_ok_and(|scope| scope == closed.scope) {
                return Err(invalid("managed_generation_retired_rebuild_required"));
            }
        }
        return Ok(None);
    };
    if locks::path_identity(generations_path)? != binding.generations {
        return Ok(None);
    }
    let parent_id = locks::path_identity(&parent)?;
    Ok(Some((root, binding, parent, parent_id)))
}
fn check_startup(
    root: &RootedDirectory,
    binding: GateBinding,
    parent_path: &Path,
    parent: FileIdentity,
) -> Result<(), NativeError> {
    if locks::path_identity(parent_path)? != parent {
        return Err(invalid("generation_startup_parent_changed"));
    }
    let directory = RootedDirectory::open_readonly_root(parent_path, parent)?;
    let (_, state) = read_gate_state(root, binding)?;
    if let GateState::Removing {
        generation,
        len,
        directory: closed,
        ..
    } = state
    {
        let (name, n) = gen_name(&generation, len)?;
        let current = root
            .open_directory_readonly(b"generations")?
            .open_directory_readonly(&name[..n]);
        if closed == parent || current.as_ref().is_ok_and(|dir| dir.identity() == parent) {
            return Err(invalid("managed_generation_retired_rebuild_required"));
        }
    }
    // A genuine FINAL alone closes future births while existing family users
    // retain their actual leases. Unknown optional provenance is not authority.
    if let Ok((origin, image, _)) = read_origin(&directory, root.identity()) {
        if origin.scope == binding.parent && read_final(&directory, origin, &image).is_ok() {
            return Err(invalid("managed_generation_retired_rebuild_required"));
        }
    }
    if let Ok((closed, _, _)) = read_legacy_final(&directory, root.identity()) {
        if closed.scope == binding.parent {
            return Err(invalid("managed_generation_retired_rebuild_required"));
        }
    }
    Ok(())
}
pub(crate) fn before_native_open(
    database: &Path,
) -> Result<Option<GenerationStartupGuard>, NativeError> {
    let Some((root, binding, parent_path, parent)) = locate_startup(database)? else {
        return Ok(None);
    };
    let file = root.open_readonly_file(START_GATE)?;
    if locks::file_identity(&file)? != binding.gate {
        return Err(invalid("generation_startup_gate_changed"));
    }
    let gate = FixedLockedFile::try_lock(file, LockMode::Shared)
        .map_err(|_| invalid("generation_startup_gate_unavailable"))?
        .ok_or_else(|| invalid("generation_startup_gate_busy"))?;
    if locks::read_coordination_incarnation(gate.file())? != Some(binding.nonce) {
        return Err(invalid("generation_startup_incarnation_changed"));
    }
    check_startup(&root, binding, &parent_path, parent)?;
    Ok(Some(GenerationStartupGuard {
        root,
        gate,
        binding,
        parent,
        parent_path,
    }))
}
pub(crate) fn recheck_before_native(
    guard: Option<&GenerationStartupGuard>,
    database: &Path,
) -> Result<(), NativeError> {
    if let Some(guard) = guard {
        let memory =
            CleanupMemory::new(64 * 1024).map_err(|_| invalid("generation_startup_memory"))?;
        let current = load_gate(&guard.root, &memory, None)?
            .ok_or_else(|| invalid("generation_startup_gate_missing"))?;
        if current.binding != guard.binding.binding
            || current.gate != guard.binding.gate
            || locks::file_identity(&guard.root.open_readonly_file(START_GATE)?)?
                != locks::file_identity(guard.gate.file())?
        {
            return Err(invalid("generation_startup_gate_replaced"));
        }
        check_startup(&guard.root, guard.binding, &guard.parent_path, guard.parent)
    } else {
        let new = before_native_open(database)?;
        drop(new);
        Ok(())
    }
}

/// Fixed transport images and recovery decisions are prepared and charged
/// before actual exclusion. Admission compares exact source stamps only.
#[derive(Clone, Copy)]
struct TransportStep {
    file: u8,
    offset: u64,
    len: usize,
    bytes: [u8; controls::INTENT_BYTES],
}
pub(crate) struct RemovalScratch {
    binding: GateBinding,
    gate_header: [u8; 72],
    source: [FixedFileStamp; 4],
    initial: GateState,
    removing: GateState,
    origin_image: [u8; ORIGIN_BYTES],
    final_image: [u8; FINAL_BYTES],
    resource: [u8; 83],
    manifest: [u8; 83],
    steps: [TransportStep; 8],
    count: usize,
    position: usize,
    recovery_end: usize,
    close_end: usize,
    readback: [u8; controls::INTENT_BYTES],
    names: [u8; 4096],
    #[cfg(test)]
    fail_after: Option<usize>,
}
impl RemovalScratch {
    pub(crate) fn new(
        root: &RootedDirectory,
        origin: Origin,
        final_record: Final,
        proof_digest: [u8; 32],
        memory: &CleanupMemory,
    ) -> Result<MemoryBox<Self>, NativeError> {
        let _temporary = memory
            .reserve(
                std::mem::size_of::<Self>() * 2
                    + controls::SELECTOR_BYTES
                    + controls::SLOT_BYTES * 3
                    + controls::INTENT_BYTES
                    + controls::COMMIT_BYTES
                    + 8192,
            )
            .map_err(|_| invalid("generation_removal_memory"))?;
        let binding_file = root.open_readonly_file(START_BINDING)?;
        let binding_stamp = stamp(&binding_file)?;
        let binding = GateBinding::decode(&exact(&binding_file)?)
            .ok_or_else(|| invalid("generation_gate_unknown"))?;
        if binding.managed != origin.managed
            || binding.parent != origin.scope
            || binding.writer != origin.writer
            || binding.binding != binding_stamp.identity()
            || root.open_directory_readonly(b"generations")?.identity() != binding.generations
        {
            return Err(invalid("generation_gate_origin_mismatch"));
        }
        let selector = root.open_readonly_file(START_SELECTOR)?;
        let selector_stamp = stamp(&selector)?;
        let image = exact::<{ controls::SELECTOR_BYTES }>(&selector)?;
        if selector_stamp.identity() != binding.selector {
            return Err(invalid("generation_gate_selector_changed"));
        }
        let mut stamps = [selector_stamp; 2];
        let mut images = [[0; controls::SLOT_BYTES]; 2];
        let mut slots = [None; 2];
        for i in 0..2 {
            let file = root.open_readonly_file(START_DATA[i])?;
            stamps[i] = stamp(&file)?;
            if stamps[i].identity() != binding.data[i] {
                return Err(invalid("generation_gate_slot_changed"));
            }
            images[i] = exact(&file)?;
            slots[i] = controls::decode_slot(&images[i], &binding.nonce).ok();
        }
        let commit = controls::decode_commit(&image[..controls::COMMIT_BYTES], &binding.nonce).ok();
        let intent = controls::decode_intent(&image[controls::COMMIT_BYTES..], &binding.nonce).ok();
        let step = controls::recovery_step(commit, intent, slots);
        let mut allocation = MemoryBox::<Self>::new_uninit(memory)
            .map_err(|_| invalid("generation_removal_memory"))?;
        let pointer = allocation.as_mut_ptr();
        // SAFETY: the charged allocation has exact Self layout. Every field is
        // initialized once, and the zeroed transport entries contain only
        // integer fields and byte arrays. No fallible operation runs until the
        // fully initialized value acquires its ordinary RAII owner below.
        unsafe {
            macro_rules! field {
                ($name:ident, $value:expr) => {
                    std::ptr::addr_of_mut!((*pointer).$name).write($value)
                };
            }
            field!(binding, binding);
            field!(
                gate_header,
                locks::encode_coordination_incarnation(binding.nonce)
            );
            field!(
                source,
                [binding_stamp, selector_stamp, stamps[0], stamps[1]]
            );
            field!(initial, GateState::Idle);
            field!(origin_image, origin.encode());
            field!(final_image, final_record.encode());
            field!(resource, resource_name());
            field!(manifest, resource_name_for(b"manifest.json"));
            field!(
                removing,
                GateState::Removing {
                    generation: origin.generation,
                    len: origin.generation_len,
                    directory: origin.directory,
                    origin: origin.receipt,
                    final_record: final_record.receipt,
                    proof_digest,
                }
            );
            std::ptr::addr_of_mut!((*pointer).steps).write_bytes(0, 1);
            field!(count, 0);
            field!(position, 0);
            field!(recovery_end, 0);
            field!(close_end, 0);
            std::ptr::addr_of_mut!((*pointer).readback).write_bytes(0, 1);
            std::ptr::addr_of_mut!((*pointer).names).write_bytes(0, 1);
            #[cfg(test)]
            field!(fail_after, None);
        }
        // SAFETY: the block above initialized every field without an unwind.
        let mut prepared = unsafe { allocation.assume_init() };
        let current = match step {
            controls::RecoveryStep::Committed(c) => c,
            controls::RecoveryStep::RewriteIntendedSlot(i)
            | controls::RecoveryStep::CommitIntendedSlot(i) => {
                let intended: &[u8; controls::SLOT_BYTES] = image[controls::COMMIT_BYTES + 96
                    ..controls::COMMIT_BYTES + 96 + controls::SLOT_BYTES]
                    .try_into()
                    .unwrap();
                if matches!(step, controls::RecoveryStep::RewriteIntendedSlot(_)) {
                    prepared.push(1 + i.target as u8, 0, intended)?;
                }
                images[i.target] = *intended;
                let next = controls::CommitFact {
                    generation: i.after,
                    active: i.target,
                    slot_digest: i.slot_digest,
                };
                let mut bytes = [0; controls::COMMIT_BYTES];
                controls::encode_commit(&mut bytes, &binding.nonce, next)
                    .map_err(|_| invalid("generation_gate_codec"))?;
                prepared.push(0, 0, &bytes)?;
                next
            }
            controls::RecoveryStep::Unknown => {
                return Err(invalid("generation_gate_recovery_unknown"))
            }
        };
        prepared.recovery_end = prepared.count;
        let slot = controls::decode_slot(&images[current.active], &binding.nonce)
            .map_err(|_| invalid("generation_gate_unknown_slot"))?;
        prepared.initial = GateState::decode(&images[current.active][56..56 + slot.payload_len])
            .ok_or_else(|| invalid("generation_gate_unknown_state"))?;
        if prepared.initial != GateState::Idle && prepared.initial != prepared.removing {
            return Err(invalid("generation_other_removal_pending"));
        }
        let removing = prepared.removing;
        let closing = if prepared.initial == GateState::Idle {
            prepared.transition(current, removing)?
        } else {
            current
        };
        prepared.close_end = prepared.count;
        prepared.transition(closing, GateState::Idle)?;
        recheck(binding_stamp, &binding_file)?;
        recheck(selector_stamp, &selector)?;
        for i in 0..2 {
            recheck(stamps[i], &root.open_readonly_file(START_DATA[i])?)?;
        }
        Ok(prepared)
    }
    /// Advisory observation only: no retirement/removal capability is minted.
    pub(crate) fn observed_committed_idle(&self) -> bool {
        self.initial == GateState::Idle && self.recovery_end == 0
    }
    #[cfg(test)]
    pub(crate) fn fail_after_transport_step(&mut self, step: usize) {
        self.fail_after = Some(step);
    }
    fn push(&mut self, file: u8, offset: u64, bytes: &[u8]) -> Result<(), NativeError> {
        if self.count == self.steps.len() || bytes.len() > controls::INTENT_BYTES {
            return Err(invalid("generation_gate_transport_capacity"));
        }
        let step = &mut self.steps[self.count];
        step.file = file;
        step.offset = offset;
        step.len = bytes.len();
        step.bytes[..bytes.len()].copy_from_slice(bytes);
        self.count += 1;
        Ok(())
    }
    fn transition(
        &mut self,
        current: controls::CommitFact,
        state: GateState,
    ) -> Result<controls::CommitFact, NativeError> {
        let generation = current
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("generation_gate_sequence_overflow"))?;
        let mut slot = [0; controls::SLOT_BYTES];
        let fact =
            controls::encode_slot(&mut slot, &self.binding.nonce, generation, &state.encode())
                .map_err(|_| invalid("generation_gate_codec"))?;
        let mut intent = [0; controls::INTENT_BYTES];
        let target = controls::encode_intent(&mut intent, &self.binding.nonce, current, &slot)
            .map_err(|_| invalid("generation_gate_codec"))?
            .target;
        let next = controls::CommitFact {
            generation,
            active: target,
            slot_digest: fact.digest,
        };
        let mut commit = [0; controls::COMMIT_BYTES];
        controls::encode_commit(&mut commit, &self.binding.nonce, next)
            .map_err(|_| invalid("generation_gate_codec"))?;
        self.push(0, controls::COMMIT_BYTES as u64, &intent)?;
        self.push(1 + target as u8, 0, &slot)?;
        self.push(0, 0, &commit)?;
        Ok(next)
    }
    fn recheck(&self, root: &RootedDirectory) -> Result<(), NativeError> {
        if self.position != 0 {
            return Err(invalid("generation_gate_preparation_consumed"));
        }
        for (i, name) in [START_BINDING, START_SELECTOR, START_DATA[0], START_DATA[1]]
            .into_iter()
            .enumerate()
        {
            recheck(self.source[i], &root.open_readonly_file(name)?)?;
        }
        Ok(())
    }
    fn run(
        &mut self,
        root: &RootedDirectory,
        end: usize,
        grant: &mut MutationGrant,
    ) -> Result<(), NativeError> {
        while self.position < end {
            #[cfg(test)]
            if self.fail_after == Some(self.position) {
                return Err(invalid("injected_generation_transport_fault"));
            }
            grant.check_time().map_err(grant_error)?;
            let step = &self.steps[self.position];
            let (name, id) = match step.file {
                0 => (START_SELECTOR, self.binding.selector),
                1 => (START_DATA[0], self.binding.data[0]),
                2 => (START_DATA[1], self.binding.data[1]),
                _ => return Err(invalid("generation_gate_transport_unknown")),
            };
            let file = root.open_existing(name)?;
            if locks::file_identity(&file)? != id {
                return Err(invalid("generation_gate_control_replaced"));
            }
            let mut cursor = &file;
            cursor.seek(SeekFrom::Start(step.offset))?;
            cursor.write_all(&step.bytes[..step.len])?;
            file.sync_all()?;
            grant.check_time().map_err(grant_error)?;
            cursor.seek(SeekFrom::Start(step.offset))?;
            cursor.read_exact(&mut self.readback[..step.len])?;
            if self.readback[..step.len] != step.bytes[..step.len] {
                return Err(invalid("generation_gate_write_changed"));
            }
            self.position += 1;
        }
        Ok(())
    }
}
fn grant_error(_: super::maintenance_budget::GrantExhausted) -> NativeError {
    invalid("generation_mutation_grant_exhausted")
}
pub(crate) struct GenerationRetirementGate {
    _lock: FixedLockedFile,
    binding: GateBinding,
}
pub(crate) fn admit_retirement_gate(
    root: &RootedDirectory,
    origin: Origin,
    scratch: &RemovalScratch,
    grant: &mut MutationGrant,
) -> Result<GenerationRetirementGate, NativeError> {
    grant.take_entries(9).map_err(grant_error)?;
    scratch.recheck(root)?;
    let binding = scratch.binding;
    if binding.managed != origin.managed
        || binding.parent != origin.scope
        || binding.writer != origin.writer
    {
        return Err(invalid("generation_gate_origin_mismatch"));
    }
    let file = root.open_readonly_file(START_GATE)?;
    if locks::file_identity(&file)? != binding.gate {
        return Err(invalid("generation_gate_replaced"));
    }
    let lock = FixedLockedFile::try_lock(file, LockMode::Exclusive)
        .map_err(|_| invalid("generation_gate_lock_invalid"))?
        .ok_or_else(|| invalid("generation_gate_busy"))?;
    if exact::<72>(lock.file())? != scratch.gate_header {
        return Err(invalid("generation_gate_incarnation_changed"));
    }
    scratch.recheck(root)?;
    Ok(GenerationRetirementGate {
        _lock: lock,
        binding,
    })
}
/// Actual empty-only removal effect; missing-name recovery is a distinct result.
pub(crate) struct RemovedGeneration {
    _directory: FileIdentity,
}
impl GenerationRetirementGate {
    // Root/directory identities, immutable origin/final proofs and the reserved grant stay separate inputs.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn remove_closed_root(
        &self,
        root: &RootedDirectory,
        directory: &RootedDirectory,
        origin: Origin,
        final_record: Final,
        proof_digest: [u8; 32],
        scratch: &mut RemovalScratch,
        grant: &mut MutationGrant,
    ) -> Result<RemovedGeneration, NativeError> {
        grant.check_time().map_err(grant_error)?;
        let removing = GateState::Removing {
            generation: origin.generation,
            len: origin.generation_len,
            directory: origin.directory,
            origin: origin.receipt,
            final_record: final_record.receipt,
            proof_digest,
        };
        if root.identity() != self.binding.managed
            || directory.identity() != origin.directory
            || final_record.directory != origin.directory
            || scratch.removing != removing
        {
            return Err(invalid("generation_removal_scope_changed"));
        }
        if scratch.initial == GateState::Idle
            && (exact::<ORIGIN_BYTES>(&directory.open_readonly_file(ORIGIN_NAME)?)?
                != scratch.origin_image
                || exact::<FINAL_BYTES>(&directory.open_readonly_file(FINAL_NAME)?)?
                    != scratch.final_image)
        {
            return Err(invalid("generation_removal_receipt_changed"));
        }
        let resource = scratch.resource;
        let manifest = scratch.manifest;
        let names: [(&[u8], Option<FileIdentity>); 7] = [
            (ORIGIN_NAME, Some(origin.receipt)),
            (FINAL_NAME, Some(final_record.receipt)),
            (b"lease.lock", Some(origin.lease)),
            (b".database-use-family.lock", Some(origin.family)),
            (&resource, Some(origin.resource)),
            (&manifest, Some(origin.manifest_resource)),
            (b"graph.ldb.lock", origin.native_lock),
        ];
        let mut seen = [false; 7];
        directory.visit_entries(&mut scratch.names, &mut |entry| {
            grant
                .check_time()
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?;
            let Some(i) = names.iter().position(|(name, _)| *name == entry) else {
                return Err(locks::directory_not_empty_error());
            };
            let Some(id) = names[i].1 else {
                return Err(std::io::ErrorKind::InvalidData.into());
            };
            if locks::file_identity(&directory.open_readonly_file(entry)?)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?
                != id
            {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            seen[i] = true;
            Ok(())
        })?;
        if scratch.initial == GateState::Idle
            && seen
                .iter()
                .zip(names)
                .any(|(seen, (_, id))| id.is_some() && !seen)
        {
            return Err(invalid("generation_removal_control_missing"));
        }
        let mut held: [Option<FixedLockedFile>; 5] = std::array::from_fn(|_| None);
        for i in 2..7 {
            if seen[i] {
                grant.check_time().map_err(grant_error)?;
                held[i - 2] = Some(
                    FixedLockedFile::try_lock(
                        directory.open_readonly_file(names[i].0)?,
                        LockMode::Exclusive,
                    )
                    .map_err(|_| invalid("generation_removal_lock_invalid"))?
                    .ok_or_else(|| invalid("generation_removal_lease_busy"))?,
                );
            }
        }
        scratch.run(root, scratch.close_end, grant)?;
        for i in 0..7 {
            if seen[i] {
                grant.check_time().map_err(grant_error)?;
                directory.unlink_file(names[i].0, names[i].1.unwrap())?;
            }
        }
        grant.check_time().map_err(grant_error)?;
        directory.sync_namespace()?;
        let generations = root.open_directory(b"generations")?;
        if generations.identity() != self.binding.generations {
            return Err(invalid("generation_namespace_replaced"));
        }
        let (name, n) = gen_name(&origin.generation, origin.generation_len)?;
        grant.check_time().map_err(grant_error)?;
        generations.remove_empty_directory(&name[..n], origin.directory)?;
        generations.sync_namespace()?;
        Ok(RemovedGeneration {
            _directory: origin.directory,
        })
    }
    pub(crate) fn complete_missing(
        &self,
        root: &RootedDirectory,
        origin: Origin,
        final_record: Final,
        proof_digest: [u8; 32],
        scratch: &mut RemovalScratch,
        grant: &mut MutationGrant,
    ) -> Result<(), NativeError> {
        if scratch.removing
            != (GateState::Removing {
                generation: origin.generation,
                len: origin.generation_len,
                directory: origin.directory,
                origin: origin.receipt,
                final_record: final_record.receipt,
                proof_digest,
            })
            || (scratch.initial == GateState::Idle && scratch.position < scratch.close_end)
        {
            return Err(invalid("generation_missing_without_removal_intent"));
        }
        scratch.run(root, scratch.recovery_end, grant)?;
        let generations = root.open_directory(b"generations")?;
        if generations.identity() != self.binding.generations {
            return Err(invalid("generation_namespace_changed"));
        }
        let (name, n) = gen_name(&origin.generation, origin.generation_len)?;
        match generations.open_directory_readonly(&name[..n]) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(invalid("generation_removal_name_reappeared")),
        }
    }
    pub(crate) fn finish_removed(
        &self,
        root: &RootedDirectory,
        origin: Origin,
        final_record: Final,
        proof_digest: [u8; 32],
        scratch: &mut RemovalScratch,
        grant: &mut MutationGrant,
    ) -> Result<(), NativeError> {
        self.complete_missing(root, origin, final_record, proof_digest, scratch, grant)?;
        scratch.run(root, scratch.count, grant)
    }
}

/// Genuine successor disposition of an unreceipted predecessor. This certifies
/// closure only; payload origin still requires complete controlled association.
pub(crate) const LEGACY_FINAL_NAME: &[u8] = b".generation-legacy-final";
#[derive(Clone, Copy)]
pub(crate) struct LegacyClosedFile {
    pub(crate) id: FileIdentity,
    pub(crate) len: u64,
    pub(crate) stamp_digest: [u8; 32],
}
#[derive(Clone, Copy)]
pub(crate) struct LegacyFinal {
    pub(crate) generation: [u8; 128],
    pub(crate) generation_len: u8,
    pub(crate) successor_id: [u8; 128],
    pub(crate) successor_len: u8,
    pub(crate) scope: GenerationOriginScope,
    pub(crate) managed: FileIdentity,
    pub(crate) writer: FileIdentity,
    pub(crate) directory: FileIdentity,
    pub(crate) receipt: FileIdentity,
    pub(crate) lease: FileIdentity,
    pub(crate) family: FileIdentity,
    pub(crate) resource: FileIdentity,
    pub(crate) manifest_resource: FileIdentity,
    pub(crate) database: LegacyClosedFile,
    pub(crate) manifest: LegacyClosedFile,
    pub(crate) successor: FileIdentity,
    pub(crate) successor_origin: FileIdentity,
    pub(crate) successor_digest: [u8; 32],
    pub(crate) operation: [u8; 32],
    pub(crate) native_lock: Option<FileIdentity>,
}
impl LegacyFinal {
    pub(crate) fn encode(self) -> [u8; 1024] {
        let mut b = [0; 1024];
        b[..8].copy_from_slice(b"CGLEGF01");
        b[8..16].copy_from_slice(&1u64.to_le_bytes());
        b[16] = self.generation_len;
        b[17] = self.successor_len;
        b[32..160].copy_from_slice(&self.generation);
        b[160..288].copy_from_slice(&self.successor_id);
        for (o, id) in [
            (288, self.scope.root),
            (304, self.scope.coordination),
            (320, self.scope.cohort),
            (336, self.scope.publication_binding),
            (384, self.managed),
            (400, self.writer),
            (416, self.directory),
            (432, self.receipt),
            (448, self.lease),
            (464, self.family),
            (480, self.resource),
            (496, self.manifest_resource),
            (512, self.database.id),
            (568, self.manifest.id),
            (624, self.successor),
            (640, self.successor_origin),
        ] {
            id_put(&mut b[o..o + 16], id);
        }
        b[352..384].copy_from_slice(&self.scope.incarnation);
        b[528..536].copy_from_slice(&self.database.len.to_le_bytes());
        b[536..568].copy_from_slice(&self.database.stamp_digest);
        b[584..592].copy_from_slice(&self.manifest.len.to_le_bytes());
        b[592..624].copy_from_slice(&self.manifest.stamp_digest);
        b[656..688].copy_from_slice(&self.successor_digest);
        b[688..720].copy_from_slice(&self.operation);
        if let Some(id) = self.native_lock {
            b[720] = 1;
            id_put(&mut b[728..744], id);
        }
        let hash = digest(&b[..992]);
        b[992..].copy_from_slice(&hash);
        b
    }
    pub(crate) fn decode(b: &[u8; 1024]) -> Option<Self> {
        if &b[..8] != b"CGLEGF01"
            || u64::from_le_bytes(b[8..16].try_into().ok()?) != 1
            || b[18..32]
                .iter()
                .chain(b[721..728].iter())
                .chain(b[744..992].iter())
                .any(|v| *v != 0)
            || b[720] > 1
            || (b[720] == 0 && b[728..744].iter().any(|v| *v != 0))
            || digest(&b[..992]) != b[992..]
            || b[352..384].iter().all(|v| *v == 0)
            || b[688..720].iter().all(|v| *v == 0)
        {
            return None;
        }
        let generation = b[32..160].try_into().ok()?;
        let successor_id = b[160..288].try_into().ok()?;
        gen_name(&generation, b[16]).ok()?;
        gen_name(&successor_id, b[17]).ok()?;
        if generation == successor_id {
            return None;
        }
        Some(Self {
            generation,
            generation_len: b[16],
            successor_id,
            successor_len: b[17],
            scope: GenerationOriginScope {
                root: id_get(&b[288..304]),
                coordination: id_get(&b[304..320]),
                cohort: id_get(&b[320..336]),
                publication_binding: id_get(&b[336..352]),
                incarnation: b[352..384].try_into().ok()?,
            },
            managed: id_get(&b[384..400]),
            writer: id_get(&b[400..416]),
            directory: id_get(&b[416..432]),
            receipt: id_get(&b[432..448]),
            lease: id_get(&b[448..464]),
            family: id_get(&b[464..480]),
            resource: id_get(&b[480..496]),
            manifest_resource: id_get(&b[496..512]),
            database: LegacyClosedFile {
                id: id_get(&b[512..528]),
                len: u64::from_le_bytes(b[528..536].try_into().ok()?),
                stamp_digest: b[536..568].try_into().ok()?,
            },
            manifest: LegacyClosedFile {
                id: id_get(&b[568..584]),
                len: u64::from_le_bytes(b[584..592].try_into().ok()?),
                stamp_digest: b[592..624].try_into().ok()?,
            },
            successor: id_get(&b[624..640]),
            successor_origin: id_get(&b[640..656]),
            successor_digest: b[656..688].try_into().ok()?,
            operation: b[688..720].try_into().ok()?,
            native_lock: (b[720] == 1).then(|| id_get(&b[728..744])),
        })
    }
}
pub(crate) fn read_legacy_final(
    directory: &RootedDirectory,
    managed: FileIdentity,
) -> Result<(LegacyFinal, [u8; 1024], FixedFileStamp), NativeError> {
    let file = directory.open_readonly_file(LEGACY_FINAL_NAME)?;
    let before = stamp(&file)?;
    let bytes = exact(&file)?;
    let fact =
        LegacyFinal::decode(&bytes).ok_or_else(|| invalid("legacy_generation_final_unknown"))?;
    if fact.managed != managed
        || fact.directory != directory.identity()
        || fact.receipt != before.identity()
    {
        return Err(invalid("legacy_generation_final_changed"));
    }
    Ok((fact, bytes, before))
}
