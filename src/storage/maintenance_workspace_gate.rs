//! Stable parent controls for closed workspace removal. The parent survives
//! child removal; its fixed intent is never stored in the child being removed.
use super::locks::{
    self, FileIdentity, FixedControlError, FixedControlProblem, LockMode, RootedDirectory,
};
use super::maintenance_budget::MemoryBox;
use super::maintenance_controls as c;
use super::maintenance_ownership::{ReferenceDomain, RootCreationAuthority};
use crate::error::NativeError;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

const DATA: [&[u8]; 2] = [b".workspace-removal.data0", b".workspace-removal.data1"];
const SELECTOR: &[u8] = b".workspace-removal.selector";
const BINDING: &[u8] = b".workspace-removal.binding";
const RECORD_BYTES: usize = 2048;
const NAMES: [&[u8]; 16] = [
    b".workspace.binding",
    b"lease.lock",
    b".workspace-writers.lock",
    b".publication.binding",
    b".publication.selector",
    b".publication.data0",
    b".publication.data1",
    b".publication.stream",
    b".workspace.domain.json",
    b".workspace.parent.json",
    b".reference-scope.json",
    b"journal.json",
    b"cleanup-barrier.json",
    b".workspace-origins",
    b"staging",
    b"candidate",
];
fn invalid() -> NativeError {
    NativeError::InvalidInput("workspace_removal_unverified".into())
}
fn fixed_invalid() -> FixedControlError {
    FixedControlError::Invalid(FixedControlProblem::InvalidRecord)
}
fn fixed_native(error: NativeError) -> FixedControlError {
    match error {
        NativeError::Io(error) => error.into(),
        _ => fixed_invalid(),
    }
}
fn id_into(bytes: &mut [u8], id: FileIdentity) {
    let (a, b) = id.fixed_words();
    bytes[..8].copy_from_slice(&a.to_le_bytes());
    bytes[8..16].copy_from_slice(&b.to_le_bytes());
}
fn id_from(bytes: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    )
}
fn image<const N: usize>(file: &std::fs::File) -> std::io::Result<[u8; N]> {
    if file.metadata()?.len() != N as u64 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = [0; N];
    locks::read_fixed_range(file, 0, &mut bytes)?;
    Ok(bytes)
}
fn create_image(
    root: &RootedDirectory,
    name: &[u8],
    bytes: &[u8],
) -> Result<FileIdentity, NativeError> {
    let mut file = root.create_new(name)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let id = locks::file_identity(&file)?;
    if file.metadata()?.len() != bytes.len() as u64 {
        return Err(invalid());
    }
    let mut read = [0u8; c::SELECTOR_BYTES];
    locks::read_fixed_range(&file, 0, &mut read[..bytes.len()])?;
    if read[..bytes.len()] != *bytes {
        return Err(invalid());
    }
    Ok(id)
}
/// Called only while genuine fresh destination and actual writer/root Exclusive
/// ownership are held. Existing names are never initialized, repaired or adopted.
pub(crate) fn initialize_fresh_workspace_gate(
    fresh: &super::layout::FreshReferenceRoot,
    writer: &locks::WriterLease,
    creation: &RootCreationAuthority<'_>,
) -> Result<(), NativeError> {
    if !writer.verifies(writer.path(), LockMode::Exclusive)?
        || locks::path_identity(fresh.path())? != fresh.identity()
    {
        return Err(invalid());
    }
    let destination = RootedDirectory::open(fresh.path(), fresh.identity())?;
    let gate_path = fresh.path().join(".workspace-startup.lock");
    let gate =
        locks::create_new_coordination_in(&destination, b".workspace-startup.lock", &gate_path)?;
    let nonce = creation.incarnation()?;
    gate.initialize_creation_incarnation(nonce)?;
    let root = RootedDirectory::open(creation.root(), creation.root_identity())?;
    let mut slot = [0; c::SLOT_BYTES];
    let fact = c::encode_slot(&mut slot, &nonce, 0, &RemovalState::Idle.encode())
        .map_err(|_| invalid())?;
    let data0 = create_image(&root, DATA[0], &slot)?;
    c::encode_slot(&mut slot, &nonce, 0, b"").map_err(|_| invalid())?;
    let data1 = create_image(&root, DATA[1], &slot)?;
    let mut selector = [0; c::SELECTOR_BYTES];
    c::encode_commit(
        &mut selector[..c::COMMIT_BYTES],
        &nonce,
        c::CommitFact {
            generation: 0,
            active: 0,
            slot_digest: fact.digest,
        },
    )
    .map_err(|_| invalid())?;
    let selector_id = create_image(&root, SELECTOR, &selector)?;
    let binding = c::PublisherBinding {
        root: creation.root_identity(),
        coordination: creation.coordination_identity()?,
        data: [data0, data1],
        selector: selector_id,
        stream: Some(gate.identity()?),
        incarnation: nonce,
    };
    let mut bytes = [0; c::BINDING_BYTES];
    binding.encode(&mut bytes).map_err(|_| invalid())?;
    root.sync_namespace()?;
    destination.sync_namespace()?;
    create_image(&root, BINDING, &bytes)?;
    root.sync_namespace()?;
    Ok(())
}

pub(super) fn legacy_bootstrap_idle_image() -> [u8; RECORD_BYTES] {
    RemovalState::Idle.encode()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RemovalSpec {
    kind: u8,
    root: FileIdentity,
    parent: FileIdentity,
    nonce: [u8; 32],
    parent_path: [u8; 32],
    name: [u8; 255],
    name_len: u8,
    source_epoch: u64,
    admission_epoch: u64,
    scope_digest: [u8; 32],
    present: u16,
    entries: [FileIdentity; 16],
    directory_nonces: [[u8; 32]; 3],
    entry_digests: [[u8; 32]; 16],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// Fixed inline protocol/lease state: boxing would add a separately accounted allocation.
#[allow(clippy::large_enum_variant)]
enum RemovalState {
    Idle,
    Removing(RemovalSpec),
}
impl RemovalState {
    fn encode(self) -> [u8; RECORD_BYTES] {
        let mut b = [0; RECORD_BYTES];
        b[..8].copy_from_slice(b"CGWREM01");
        if let Self::Removing(s) = self {
            b[8] = 1;
            b[9] = s.kind;
            b[10] = s.name_len;
            id_into(&mut b[16..32], s.root);
            id_into(&mut b[32..48], s.parent);
            b[48..80].copy_from_slice(&s.nonce);
            b[80..112].copy_from_slice(&s.parent_path);
            b[112..120].copy_from_slice(&s.source_epoch.to_le_bytes());
            b[120..128].copy_from_slice(&s.admission_epoch.to_le_bytes());
            b[128..160].copy_from_slice(&s.scope_digest);
            b[160..162].copy_from_slice(&s.present.to_le_bytes());
            b[168..423].copy_from_slice(&s.name);
            for (i, id) in s.entries.into_iter().enumerate() {
                id_into(&mut b[424 + i * 16..440 + i * 16], id);
            }
            for (i, n) in s.directory_nonces.into_iter().enumerate() {
                b[680 + i * 32..712 + i * 32].copy_from_slice(&n);
            }
            for (i, digest) in s.entry_digests.into_iter().enumerate() {
                b[776 + i * 32..808 + i * 32].copy_from_slice(&digest);
            }
        }
        b
    }
    fn decode(b: &[u8]) -> Option<Self> {
        if b.len() != RECORD_BYTES || &b[..8] != b"CGWREM01" {
            return None;
        }
        if b[8] == 0 {
            return b[9..].iter().all(|x| *x == 0).then_some(Self::Idle);
        }
        if b[8] != 1
            || !matches!(b[9], 1..=3)
            || b[10] == 0
            || b[11..16]
                .iter()
                .chain(&b[162..168])
                .chain(&b[423..424])
                .chain(&b[1288..])
                .any(|x| *x != 0)
        {
            return None;
        }
        let mut name = [0; 255];
        name.copy_from_slice(&b[168..423]);
        let n = b[10] as usize;
        if name[n..].iter().any(|x| *x != 0)
            || name[..n].iter().any(|x| matches!(*x, 0 | b'/' | b'\\'))
        {
            return None;
        }
        let entries = std::array::from_fn(|i| id_from(&b[424 + i * 16..440 + i * 16]));
        let directory_nonces =
            std::array::from_fn(|i| b[680 + i * 32..712 + i * 32].try_into().unwrap());
        Some(Self::Removing(RemovalSpec {
            kind: b[9],
            root: id_from(&b[16..32]),
            parent: id_from(&b[32..48]),
            nonce: b[48..80].try_into().unwrap(),
            parent_path: b[80..112].try_into().unwrap(),
            name,
            name_len: b[10],
            source_epoch: u64::from_le_bytes(b[112..120].try_into().unwrap()),
            admission_epoch: u64::from_le_bytes(b[120..128].try_into().unwrap()),
            scope_digest: b[128..160].try_into().unwrap(),
            present: u16::from_le_bytes(b[160..162].try_into().unwrap()),
            entries,
            directory_nonces,
            entry_digests: std::array::from_fn(|i| {
                b[776 + i * 32..808 + i * 32].try_into().unwrap()
            }),
        }))
    }
}

struct WriteStep {
    file: usize,
    offset: u64,
    start: usize,
    len: usize,
}
struct Transport {
    binding: c::PublisherBinding,
    source: [locks::FixedFileStamp; 4],
    commit: c::CommitFact,
    state: RemovalState,
    steps: [Option<WriteStep>; 8],
    count: usize,
    recovery_end: usize,
    remove_end: usize,
    bytes: super::maintenance_budget::ReservedBytes,
}
impl Transport {
    fn prepare(
        root: &RootedDirectory,
        scope: (FileIdentity, FileIdentity, [u8; 32], FileIdentity),
        startup: FileIdentity,
        memory: &super::maintenance_budget::CleanupMemory,
    ) -> Result<Self, FixedControlError> {
        let bytes = memory
            .reserve(8 * c::SELECTOR_BYTES)
            .map_err(FixedControlError::Resource)?
            .bytes(8 * c::SELECTOR_BYTES)
            .map_err(FixedControlError::Resource)?;
        let binding_file = root.open_readonly_file(BINDING)?;
        let binding = c::PublisherBinding::decode(&image::<{ c::BINDING_BYTES }>(&binding_file)?)
            .map_err(|_| fixed_invalid())?;
        if (binding.root, binding.coordination, binding.incarnation) != (scope.0, scope.1, scope.2)
            || binding.stream != Some(startup)
        {
            return Err(fixed_invalid());
        }
        let selector_file = root.open_readonly_file(SELECTOR)?;
        let d0 = root.open_readonly_file(DATA[0])?;
        let d1 = root.open_readonly_file(DATA[1])?;
        if locks::file_identity(&selector_file).map_err(fixed_native)? != binding.selector
            || locks::file_identity(&d0).map_err(fixed_native)? != binding.data[0]
            || locks::file_identity(&d1).map_err(fixed_native)? != binding.data[1]
        {
            return Err(fixed_invalid());
        }
        let source = [
            locks::FixedFileStamp::capture(&binding_file, locks::FixedControlKind::File)?,
            locks::FixedFileStamp::capture(&selector_file, locks::FixedControlKind::File)?,
            locks::FixedFileStamp::capture(&d0, locks::FixedControlKind::File)?,
            locks::FixedFileStamp::capture(&d1, locks::FixedControlKind::File)?,
        ];
        let selector = image::<{ c::SELECTOR_BYTES }>(&selector_file)?;
        let data = [
            image::<{ c::SLOT_BYTES }>(&d0)?,
            image::<{ c::SLOT_BYTES }>(&d1)?,
        ];
        for (stamp, file) in source.iter().zip([&binding_file, &selector_file, &d0, &d1]) {
            stamp.recheck(file)?;
        }
        let commit = c::decode_commit(&selector[..c::COMMIT_BYTES], &scope.2).ok();
        let intent = c::decode_intent(&selector[c::COMMIT_BYTES..], &scope.2).ok();
        let slots = [
            c::decode_slot(&data[0], &scope.2).ok(),
            c::decode_slot(&data[1], &scope.2).ok(),
        ];
        let recovery = c::recovery_step(commit, intent, slots);
        let (current, image) = match recovery {
            c::RecoveryStep::Committed(current) => (current, data[current.active]),
            c::RecoveryStep::RewriteIntendedSlot(intent)
            | c::RecoveryStep::CommitIntendedSlot(intent) => {
                let mut intended = [0; c::SLOT_BYTES];
                intended.copy_from_slice(
                    &selector[c::COMMIT_BYTES + 96..c::COMMIT_BYTES + 96 + c::SLOT_BYTES],
                );
                (
                    c::CommitFact {
                        generation: intent.after,
                        active: intent.target,
                        slot_digest: intent.slot_digest,
                    },
                    intended,
                )
            }
            c::RecoveryStep::Unknown => return Err(fixed_invalid()),
        };
        let len = u64::from_le_bytes(image[48..56].try_into().unwrap()) as usize;
        let state = RemovalState::decode(&image[56..56 + len]).ok_or_else(fixed_invalid)?;
        let mut result = Self {
            binding,
            source,
            commit: current,
            state,
            steps: std::array::from_fn(|_| None),
            count: 0,
            recovery_end: 0,
            remove_end: 0,
            bytes,
        };
        match recovery {
            c::RecoveryStep::RewriteIntendedSlot(_) => {
                result.push(1 + current.active, 0, &image)?;
                result.push_commit(current)?;
            }
            c::RecoveryStep::CommitIntendedSlot(_) => result.push_commit(current)?,
            _ => {}
        }
        result.recovery_end = result.count;
        Ok(result)
    }
    fn push(&mut self, file: usize, offset: u64, image: &[u8]) -> Result<(), FixedControlError> {
        if self.count == 8 || image.len() > c::SELECTOR_BYTES {
            return Err(fixed_invalid());
        }
        let start = self.count * c::SELECTOR_BYTES;
        self.bytes.as_mut_slice()[start..start + image.len()].copy_from_slice(image);
        self.steps[self.count] = Some(WriteStep {
            file,
            offset,
            start,
            len: image.len(),
        });
        self.count += 1;
        Ok(())
    }
    fn push_commit(&mut self, commit: c::CommitFact) -> Result<(), FixedControlError> {
        let mut image = [0; c::COMMIT_BYTES];
        c::encode_commit(&mut image, &self.binding.incarnation, commit)
            .map_err(|_| fixed_invalid())?;
        self.push(0, 0, &image)
    }
    fn plan(&mut self, state: RemovalState) -> Result<(), FixedControlError> {
        let generation = self
            .commit
            .generation
            .checked_add(1)
            .ok_or_else(fixed_invalid)?;
        let mut slot = [0; c::SLOT_BYTES];
        let fact = c::encode_slot(
            &mut slot,
            &self.binding.incarnation,
            generation,
            &state.encode(),
        )
        .map_err(|_| fixed_invalid())?;
        let mut intent = [0; c::INTENT_BYTES];
        let next = c::encode_intent(&mut intent, &self.binding.incarnation, self.commit, &slot)
            .map_err(|_| fixed_invalid())?;
        self.push(0, c::COMMIT_BYTES as u64, &intent)?;
        self.push(1 + next.target, 0, &slot)?;
        self.commit = c::CommitFact {
            generation,
            active: next.target,
            slot_digest: fact.digest,
        };
        self.push_commit(self.commit)?;
        Ok(())
    }
    fn run(
        &mut self,
        root: &RootedDirectory,
        selector: &std::fs::File,
        start: usize,
        end: usize,
        grant: &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), FixedControlError> {
        for index in start..end {
            grant.check_time()?;
            let step = self.steps[index].as_ref().ok_or_else(fixed_invalid)?;
            let (name, id) = match step.file {
                0 => (SELECTOR, self.binding.selector),
                1 => (DATA[0], self.binding.data[0]),
                2 => (DATA[1], self.binding.data[1]),
                _ => return Err(fixed_invalid()),
            };
            let opened;
            let file = if step.file == 0 {
                selector
            } else {
                opened = root.open_existing(name)?;
                &opened
            };
            if locks::file_identity(file).map_err(fixed_native)? != id {
                return Err(fixed_invalid());
            }
            locks::write_fixed_range(
                file,
                step.offset,
                &self.bytes.as_slice()[step.start..step.start + step.len],
                grant,
            )?;
            let mut verify = [0; c::SELECTOR_BYTES];
            locks::read_fixed_range(file, step.offset, &mut verify[..step.len])?;
            if verify[..step.len] != self.bytes.as_slice()[step.start..step.start + step.len] {
                return Err(fixed_invalid());
            }
        }
        Ok(())
    }
}

pub(crate) struct PreparedWorkspaceRemoval {
    root: PathBuf,
    parent: PathBuf,
    parent_root: PathBuf,
    gate: PathBuf,
    gate_stamp: locks::FixedFileStamp,
    spec: RemovalSpec,
    transport: MemoryBox<Transport>,
    initial: bool,
    usable: bool,
    observed: [Option<locks::FixedFileStamp>; 16],
    present_now: u16,
    root_was_present: bool,
    epoch: MemoryBox<super::maintenance_admission::PreparedScopedInvalidation>,
    scratch: MemoryBox<locks::FixedControlScratch>,
    visit: super::maintenance_budget::ReservedBytes,
    _root_pin: Option<std::fs::File>,
    _parent_pin: std::fs::File,
    _memory: super::maintenance_budget::MemoryReservation,
}
fn file_digest(
    file: &std::fs::File,
) -> Result<([u8; 32], locks::FixedFileStamp), FixedControlError> {
    let stamp = locks::FixedFileStamp::capture(file, locks::FixedControlKind::File)?;
    if stamp.len() > 8 * 1024 * 1024 {
        return Err(fixed_invalid());
    }
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 8192];
    let mut offset = 0;
    while offset < stamp.len() {
        let count = (stamp.len() - offset).min(bytes.len() as u64) as usize;
        locks::read_fixed_range(file, offset, &mut bytes[..count])?;
        hash.update(&bytes[..count]);
        offset += count as u64;
    }
    stamp.recheck(file)?;
    Ok((hash.finalize().into(), stamp))
}
fn empty(directory: &RootedDirectory, bytes: &mut [u8; 4096]) -> Result<(), FixedControlError> {
    let mut found = false;
    directory
        .visit_entries(bytes, &mut |_| {
            found = true;
            Err(std::io::ErrorKind::Interrupted.into())
        })
        .or_else(|error| {
            if found && error.kind() == std::io::ErrorKind::Interrupted {
                Ok(())
            } else {
                Err(error)
            }
        })?;
    if found {
        Err(FixedControlError::Invalid(FixedControlProblem::Busy))
    } else {
        Ok(())
    }
}
fn known_names(
    directory: &RootedDirectory,
    bytes: &mut [u8; 4096],
) -> Result<(), FixedControlError> {
    directory.visit_entries(bytes, &mut |name| {
        if NAMES.contains(&name) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::InvalidData.into())
        }
    })?;
    Ok(())
}
#[inline(never)]
fn boxed_removal_scratch(
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<MemoryBox<locks::FixedControlScratch>, FixedControlError> {
    let scratch = locks::FixedControlScratch::new(
        memory
            .reserve(locks::FixedControlScratch::required_bytes())
            .map_err(FixedControlError::Resource)?,
    )?;
    MemoryBox::new(scratch, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_removal_epoch(
    domain: &ReferenceDomain,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<MemoryBox<super::maintenance_admission::PreparedScopedInvalidation>, FixedControlError>
{
    let epoch = super::maintenance_admission::prepare_scoped_invalidation(domain, memory)?;
    MemoryBox::new(epoch, memory).map_err(FixedControlError::Resource)
}
#[inline(never)]
fn boxed_removal_transport(
    parent: &RootedDirectory,
    reference: (FileIdentity, FileIdentity, [u8; 32], FileIdentity),
    gate: FileIdentity,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<MemoryBox<Transport>, FixedControlError> {
    let transport = Transport::prepare(parent, reference, gate, memory)?;
    MemoryBox::new(transport, memory).map_err(FixedControlError::Resource)
}

/// The optional terminal argument is required to create a NEW removal intent.
/// Replay accepts only the already-durable exact parent intent. Neither path
/// adopts a directory from its name, numeric identity, or an Empty observation.
// Retain the existing value API for standalone callers; joined-worker paths use the charged factory.
#[allow(dead_code)]
pub(crate) fn prepare_workspace_root_removal(
    root: &Path,
    domain: &ReferenceDomain,
    terminal: Option<&c::PreparedTerminalWorkspace>,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<PreparedWorkspaceRemoval, FixedControlError> {
    prepare_workspace_root_removal_boxed(root, domain, terminal, memory).map(MemoryBox::into_inner)
}

/// The same exact source/intent preparation, returned behind its charged owner
/// before the caller enters another bounded stage on the joined worker.
#[inline(never)]
pub(crate) fn prepare_workspace_root_removal_boxed(
    root: &Path,
    domain: &ReferenceDomain,
    terminal: Option<&c::PreparedTerminalWorkspace>,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<MemoryBox<PreparedWorkspaceRemoval>, FixedControlError> {
    let path_bytes = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(domain.artifact_root().as_os_str().as_encoded_bytes().len())
        .and_then(|x| x.checked_add(2048))
        .and_then(|x| x.checked_mul(20))
        .and_then(|x| x.checked_add(32768))
        .ok_or(FixedControlError::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))?;
    let reservation = memory
        .reserve(path_bytes)
        .map_err(FixedControlError::Resource)?;
    let mut scratch = boxed_removal_scratch(memory)?;
    let mut visit = memory
        .reserve(4096)
        .map_err(FixedControlError::Resource)?
        .bytes(4096)
        .map_err(FixedControlError::Resource)?;
    let epoch = boxed_removal_epoch(domain, memory)?;
    let reference = epoch.root_facts();
    let parent_root = domain.artifact_root().to_owned();
    let parent_controls = RootedDirectory::open_readonly_root(&parent_root, reference.0)?;
    let gate = domain.workspace_startup_path();
    let gate_file =
        locks::open_fixed_control(&gate, locks::FixedControlKind::File, false, &mut scratch)?;
    let gate_stamp = locks::FixedFileStamp::capture(&gate_file, locks::FixedControlKind::File)?;
    if locks::read_coordination_incarnation(&gate_file).map_err(fixed_native)? != Some(reference.2)
    {
        return Err(fixed_invalid());
    }
    let mut transport =
        boxed_removal_transport(&parent_controls, reference, gate_stamp.identity(), memory)?;
    let parent = root.parent().ok_or_else(fixed_invalid)?.to_owned();
    let parent_id = locks::path_identity(&parent).map_err(fixed_native)?;
    let parent_directory = RootedDirectory::open_readonly_root(&parent, parent_id)?;
    let name = root
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(fixed_invalid)?
        .as_bytes();
    if name.is_empty() || name.len() > 255 || !name.is_ascii() {
        return Err(fixed_invalid());
    }
    let parent_path: [u8; 32] = Sha256::digest(parent.as_os_str().as_encoded_bytes()).into();
    let initial = transport.state == RemovalState::Idle;
    let mut spec = match transport.state {
        RemovalState::Removing(spec) => spec,
        RemovalState::Idle => {
            let terminal = terminal.ok_or_else(fixed_invalid)?;
            let scope = terminal.removal_scope();
            if terminal.root() != root
                || scope.parent_reference != reference
                || scope.startup_gate != gate_stamp.identity()
                || scope.parent != parent_id
            {
                return Err(fixed_invalid());
            }
            let mut encoded_name = [0; 255];
            encoded_name[..name.len()].copy_from_slice(name);
            RemovalSpec {
                kind: scope.kind,
                root: scope.root,
                parent: parent_id,
                nonce: scope.nonce,
                parent_path,
                name: encoded_name,
                name_len: name.len() as u8,
                source_epoch: terminal.source_epoch(),
                admission_epoch: terminal.admission_epoch(),
                scope_digest: terminal.scope_digest(),
                present: 0,
                entries: [FileIdentity::from_fixed_words(0, 0); 16],
                directory_nonces: scope.directories.map(|x| x.map_or([0; 32], |x| x.1)),
                entry_digests: [[0; 32]; 16],
            }
        }
    };
    if spec.parent != parent_id
        || spec.parent_path != parent_path
        || &spec.name[..spec.name_len as usize] != name
    {
        return Err(fixed_invalid());
    }
    let root_directory = match parent_directory.open_directory_readonly(name) {
        Ok(root) => Some(root),
        Err(error) if !initial && error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let mut observed = [None; 16];
    let mut present_now = 0u16;
    if let Some(directory) = &root_directory {
        if directory.identity() != spec.root {
            return Err(fixed_invalid());
        }
        known_names(
            directory,
            visit
                .as_mut_slice()
                .try_into()
                .map_err(|_| fixed_invalid())?,
        )?;
        for (index, name) in NAMES.into_iter().enumerate() {
            if index >= 13 {
                match directory.open_directory_readonly(name) {
                    Ok(child) => {
                        present_now |= 1 << index;
                        let id = child.identity();
                        let nonce = locks::created_file_nonce(&child.pin_handle()?)?;
                        if initial {
                            let wanted = terminal.unwrap().removal_scope().directories[index - 13]
                                .ok_or_else(fixed_invalid)?;
                            if (id, nonce) != (wanted.0, Some(wanted.1)) {
                                return Err(fixed_invalid());
                            }
                            spec.present |= 1 << index;
                            spec.entries[index] = id;
                        } else if spec.present & (1 << index) == 0
                            || spec.entries[index] != id
                            || nonce != Some(spec.directory_nonces[index - 13])
                        {
                            return Err(fixed_invalid());
                        }
                        empty(
                            &child,
                            visit
                                .as_mut_slice()
                                .try_into()
                                .map_err(|_| fixed_invalid())?,
                        )?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        if initial
                            && terminal.unwrap().removal_scope().directories[index - 13].is_some()
                        {
                            return Err(fixed_invalid());
                        }
                    }
                    Err(error) => return Err(error.into()),
                }
            } else {
                match directory.open_readonly_file(name) {
                    Ok(file) => {
                        present_now |= 1 << index;
                        let (digest, stamp) = file_digest(&file)?;
                        if initial {
                            spec.present |= 1 << index;
                            spec.entries[index] = stamp.identity();
                            spec.entry_digests[index] = digest;
                        } else if spec.present & (1 << index) == 0
                            || spec.entries[index] != stamp.identity()
                            || spec.entry_digests[index] != digest
                        {
                            return Err(fixed_invalid());
                        }
                        observed[index] = Some(stamp);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if initial
            && (spec.present & 0x7f != 0x7f
                || spec.present & (1 << 8) == 0
                || spec.present & (1 << 13) == 0)
        {
            return Err(fixed_invalid());
        }
    }
    if initial {
        transport.plan(RemovalState::Removing(spec))?;
        transport.remove_end = transport.count;
    } else {
        transport.remove_end = transport.recovery_end;
        transport.plan(RemovalState::Idle)?;
    }
    MemoryBox::new(
        PreparedWorkspaceRemoval {
            root: root.to_owned(),
            parent,
            parent_root,
            gate,
            gate_stamp,
            spec,
            transport,
            initial,
            usable: true,
            observed,
            present_now,
            root_was_present: root_directory.is_some(),
            epoch,
            scratch,
            visit,
            _root_pin: root_directory
                .as_ref()
                .map(|d| d.pin_handle())
                .transpose()?,
            _parent_pin: parent_directory.pin_handle()?,
            _memory: reservation,
        },
        memory,
    )
    .map_err(FixedControlError::Resource)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceRemovalProgress {
    Pending,
    Complete,
}
/// This variant is constructed ONLY after this invocation's actual empty rmdir.
pub(crate) struct RemovedWorkspace {
    root: FileIdentity,
    nonce: [u8; 32],
    source: u64,
}
/// Recovery fact is deliberately distinct: durable exact Removing intent plus
/// the same parent namespace and an absent exact name, under actual gate Excl.
pub(crate) struct RecoveredMissingWorkspace {
    root: FileIdentity,
    nonce: [u8; 32],
    source: u64,
}
pub(crate) enum WorkspaceRootOutcome<'a> {
    Removed(&'a RemovedWorkspace),
    RecoveredMissing(&'a RecoveredMissingWorkspace),
}
impl WorkspaceRootOutcome<'_> {
    pub(crate) fn root_identity(&self) -> FileIdentity {
        match self {
            Self::Removed(x) => x.root,
            Self::RecoveredMissing(x) => x.root,
        }
    }
    pub(crate) fn incarnation(&self) -> [u8; 32] {
        match self {
            Self::Removed(x) => x.nonce,
            Self::RecoveredMissing(x) => x.nonce,
        }
    }
    pub(crate) fn source_epoch(&self) -> u64 {
        match self {
            Self::Removed(x) => x.source,
            Self::RecoveredMissing(x) => x.source,
        }
    }
}
impl PreparedWorkspaceRemoval {
    fn open_parent_controls(
        path: &Path,
        transport: &Transport,
        scratch: &mut locks::FixedControlScratch,
        grant: &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(RootedDirectory, locks::FixedLockedFile), FixedControlError> {
        grant.take_entry()?;
        let root = RootedDirectory::open_fixed(path, transport.binding.root, true, scratch)?;
        grant.take_entry()?;
        let file = root.open_existing(SELECTOR)?;
        transport.source[1].recheck(&file)?;
        let selector = locks::FixedLockedFile::try_lock(file, LockMode::Exclusive)?
            .ok_or(FixedControlError::Invalid(FixedControlProblem::Busy))?;
        // The held selector cannot change beneath the already prepared images.
        for (index, name) in [(0, BINDING), (2, DATA[0]), (3, DATA[1])] {
            grant.take_entry()?;
            transport.source[index].recheck(&root.open_readonly_file(name)?)?;
        }
        Ok((root, selector))
    }
}
/// Initial intent is emitted only inside the real terminal callback, retaining
/// actual global startup/owner/writer/selector guards before its first byte.
pub(crate) fn begin_workspace_root_removal(
    terminal: &mut c::PreparedTerminalWorkspace,
    prepared: &mut PreparedWorkspaceRemoval,
    grant: &mut super::maintenance_budget::MutationGrant,
) -> Result<WorkspaceRemovalProgress, FixedControlError> {
    if !prepared.initial
        || !prepared.usable
        || terminal.root() != prepared.root
        || terminal.scope_digest() != prepared.spec.scope_digest
    {
        return Err(fixed_invalid());
    }
    // Fixed selector/check admission occurs BEFORE terminal's own E advance.
    let (parent, _selector) = PreparedWorkspaceRemoval::open_parent_controls(
        &prepared.parent_root,
        &prepared.transport,
        &mut prepared.scratch,
        grant,
    )?;
    let effects = prepared
        .transport
        .remove_end
        .checked_mul(2)
        .ok_or_else(fixed_invalid)?;
    let spec = prepared.spec;
    c::with_terminal_workspace(terminal, grant, effects, &mut |lease, grant| {
        let scope = lease.removal_scope();
        if (
            scope.root,
            scope.nonce,
            lease.source_epoch(),
            lease.admission_epoch(),
        ) != (
            spec.root,
            spec.nonce,
            spec.source_epoch,
            spec.admission_epoch,
        ) {
            return Err(fixed_invalid());
        }
        prepared.usable = false;
        let remove_end = prepared.transport.remove_end;
        prepared
            .transport
            .run(&parent, _selector.file(), 0, remove_end, grant)
    })?;
    Ok(WorkspaceRemovalProgress::Pending)
}

/// Advance a previously durable exact Removing intent. Each call reserves at
/// most32 effects, touches only the fixed16 metadata names, and retains parent
/// controls/actual startup gate throughout. Payload/receipt removal is NOT an
/// operation this API can perform. Bulk Work remains borrowed and offslot.
pub(crate) fn advance_workspace_root_removal(
    prepared: &mut PreparedWorkspaceRemoval,
    grant: &mut super::maintenance_budget::MutationGrant,
    done_entries: usize,
    done: &mut impl FnMut(
        &WorkspaceRootOutcome<'_>,
        &mut super::maintenance_budget::MutationGrant,
    ) -> Result<(), FixedControlError>,
) -> Result<WorkspaceRemovalProgress, FixedControlError> {
    if prepared.initial || !prepared.usable || done_entries > 16 {
        return Err(fixed_invalid());
    }
    // A full root's fixed admission checks leave less than32 effect entries.
    // Remove a smaller batch first; once absent, the shorter admission can
    // reserve the larger completion callback and durable reset allowance.
    let effect_entries = if prepared.root_was_present { 16 } else { 32 };
    let (parent_controls, _selector) = PreparedWorkspaceRemoval::open_parent_controls(
        &prepared.parent_root,
        &prepared.transport,
        &mut prepared.scratch,
        grant,
    )?;
    grant.take_entry()?;
    let gate_file = locks::open_fixed_control(
        &prepared.gate,
        locks::FixedControlKind::File,
        false,
        &mut prepared.scratch,
    )?;
    prepared.gate_stamp.recheck(&gate_file)?;
    let _gate = locks::FixedLockedFile::try_lock(gate_file, LockMode::Exclusive)?
        .ok_or(FixedControlError::Invalid(FixedControlProblem::Busy))?;
    if !_gate.matches_fixed_header(&locks::encode_coordination_incarnation(
        prepared.transport.binding.incarnation,
    ))? {
        return Err(fixed_invalid());
    }
    grant.take_entry()?;
    let parent = RootedDirectory::open_fixed(
        &prepared.parent,
        prepared.spec.parent,
        true,
        &mut prepared.scratch,
    )?;
    grant.take_entry()?;
    let name = &prepared.spec.name[..prepared.spec.name_len as usize];
    let root = match parent.open_directory(name) {
        Ok(root) if prepared.root_was_present && root.identity() == prepared.spec.root => {
            Some(root)
        }
        Err(error)
            if !prepared.root_was_present && error.kind() == std::io::ErrorKind::NotFound =>
        {
            None
        }
        Err(error) => return Err(error.into()),
        _ => return Err(fixed_invalid()),
    };
    let mut guards: [Option<locks::FixedLockedFile>; 3] = std::array::from_fn(|_| None);
    if let Some(root) = &root {
        for (slot, index) in [1usize, 2, 4].into_iter().enumerate() {
            if prepared.present_now & (1 << index) != 0 {
                grant.take_entry()?;
                let file = root.open_existing(NAMES[index])?;
                prepared.observed[index]
                    .ok_or_else(fixed_invalid)?
                    .recheck(&file)?;
                let guard = locks::FixedLockedFile::try_lock(file, LockMode::Exclusive)?
                    .ok_or(FixedControlError::Invalid(FixedControlProblem::Busy))?;
                if matches!(index, 1 | 2)
                    && !guard.matches_fixed_header(&locks::encode_coordination_incarnation(
                        prepared.spec.nonce,
                    ))?
                {
                    return Err(fixed_invalid());
                }
                guards[slot] = Some(guard);
            }
        }
    }
    let PreparedWorkspaceRemoval {
        spec,
        present_now,
        observed,
        epoch,
        transport,
        visit,
        usable,
        ..
    } = prepared;
    let spec = *spec;
    let present_now = *present_now;
    let mut progress = WorkspaceRemovalProgress::Pending;
    super::maintenance_admission::with_scoped_reference_invalidation(
        epoch,
        grant,
        effect_entries,
        &mut |_, grant| {
            if let Some(root) = &root {
                grant.take_entry()?;
                let bytes: &mut [u8; 4096] = visit
                    .as_mut_slice()
                    .try_into()
                    .map_err(|_| fixed_invalid())?;
                root.visit_entries(bytes, &mut |name| {
                    grant
                        .take_entry()
                        .map_err(|_| std::io::Error::from(std::io::ErrorKind::WouldBlock))?;
                    if NAMES.contains(&name) {
                        Ok(())
                    } else {
                        Err(std::io::ErrorKind::InvalidData.into())
                    }
                })?;
                for (index, name) in NAMES.into_iter().enumerate() {
                    grant.take_entry()?;
                    if index < 13 {
                        match (root.open_readonly_file(name), observed[index]) {
                            (Ok(file), Some(stamp)) => stamp.recheck(&file)?,
                            (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => {}
                            (Err(error), _) => return Err(error.into()),
                            _ => return Err(fixed_invalid()),
                        }
                    } else {
                        match root.open_directory_readonly(name) {
                            Ok(child) if present_now & (1 << index) != 0 => {
                                if child.identity() != spec.entries[index]
                                    || locks::created_file_nonce(&child.pin_handle()?)?
                                        != Some(spec.directory_nonces[index - 13])
                                {
                                    return Err(fixed_invalid());
                                }
                                grant.take_entry()?;
                                empty(&child, bytes)?;
                            }
                            Err(error)
                                if present_now & (1 << index) == 0
                                    && error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => return Err(error.into()),
                            _ => return Err(fixed_invalid()),
                        }
                    }
                }
            } else {
                grant.take_entry()?;
                match parent.open_directory_readonly(&spec.name[..spec.name_len as usize]) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                    Ok(_) => return Err(fixed_invalid()),
                }
            }
            Ok(())
        },
        &mut |_, grant| {
            *usable = false;
            let mut left = effect_entries;
            let recovery = transport.recovery_end * 2;
            if recovery > left {
                return Err(fixed_invalid());
            }
            let recovery_end = transport.recovery_end;
            transport.run(&parent_controls, _selector.file(), 0, recovery_end, grant)?;
            left -= recovery;
            let mut removed = None;
            if let Some(root) = &root {
                // Empty declared directories first; selectors and stable locks last.
                for index in [13usize, 14, 15, 8, 9, 10, 11, 12, 7, 5, 6, 3, 0, 4, 2, 1] {
                    if present_now & (1 << index) == 0 {
                        continue;
                    }
                    if left < 3 {
                        return Ok(());
                    }
                    grant.check_time()?;
                    if let Some(slot) = match index {
                        1 => Some(0),
                        2 => Some(1),
                        4 => Some(2),
                        _ => None,
                    } {
                        drop(guards[slot].take());
                    }
                    if index >= 13 {
                        root.remove_empty_directory(NAMES[index], spec.entries[index])?;
                    } else {
                        root.unlink_file(NAMES[index], spec.entries[index])?;
                    }
                    grant.check_time()?;
                    root.sync_namespace()?;
                    left -= 3;
                }
                if left < 3 {
                    return Ok(());
                }
                grant.check_time()?;
                parent.remove_empty_directory(&spec.name[..spec.name_len as usize], spec.root)?;
                parent.sync_namespace()?;
                left -= 3;
                removed = Some(RemovedWorkspace {
                    root: spec.root,
                    nonce: spec.nonce,
                    source: spec.source_epoch,
                });
            }
            // If the budget cannot cover durable S3Done + parent reset, leave the
            // exact Removing intent. Fresh replay yields RecoveredMissing, never a
            // fabricated actual-rmdir event.
            let reset = (transport.count - transport.recovery_end) * 2;
            if left < done_entries + reset {
                return Ok(());
            }
            grant.check_time()?;
            if let Some(removed) = &removed {
                done(&WorkspaceRootOutcome::Removed(removed), grant)?;
            } else {
                let missing = RecoveredMissingWorkspace {
                    root: spec.root,
                    nonce: spec.nonce,
                    source: spec.source_epoch,
                };
                done(&WorkspaceRootOutcome::RecoveredMissing(&missing), grant)?;
            }
            let reset_start = transport.recovery_end;
            let reset_end = transport.count;
            transport.run(
                &parent_controls,
                _selector.file(),
                reset_start,
                reset_end,
                grant,
            )?;
            progress = WorkspaceRemovalProgress::Complete;
            Ok(())
        },
    )?;
    Ok(progress)
}

impl PreparedWorkspaceRemoval {
    pub(crate) fn is_initial(&self) -> bool {
        self.initial
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}
/// Main runtime supplies its actual configured StateRoot for Direct runs;
/// Managed and raw-pair worker/phase registries derive from the exact domain.
/// The durable parent-path hash+identity still have to match the resulting path.
pub(crate) fn prepare_pending_workspace_root_removal_for_target(
    target: &super::inventory::MaintenanceTarget,
    domain: &ReferenceDomain,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<PreparedWorkspaceRemoval>>, FixedControlError>
{
    prepare_pending_workspace_root_removal_in(domain, Some(&target.state_root), memory)
}
#[cfg(test)]
pub(crate) fn prepare_pending_workspace_root_removal(
    domain: &ReferenceDomain,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<PreparedWorkspaceRemoval>>, FixedControlError>
{
    prepare_pending_workspace_root_removal_in(domain, None, memory)
}
#[inline(never)]
fn prepare_pending_workspace_root_removal_in(
    domain: &ReferenceDomain,
    state_root: Option<&Path>,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<Option<super::maintenance_budget::MemoryBox<PreparedWorkspaceRemoval>>, FixedControlError>
{
    let Some(path) = pending_workspace_path(domain, state_root, memory)? else {
        return Ok(None);
    };
    prepare_workspace_root_removal_boxed(&path, domain, None, memory).map(Some)
}
#[inline(never)]
fn pending_workspace_path(
    domain: &ReferenceDomain,
    state_root: Option<&Path>,
    memory: &super::maintenance_budget::CleanupMemory,
) -> Result<Option<PathBuf>, FixedControlError> {
    let size = domain
        .artifact_root()
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(state_root.map_or(0, |x| x.as_os_str().as_encoded_bytes().len()))
        .and_then(|x| x.checked_mul(8))
        .and_then(|x| x.checked_add(16384))
        .ok_or(FixedControlError::Resource(
            super::maintenance_budget::ResourceLimit::ArithmeticOverflow,
        ))?;
    let reservation = memory.reserve(size).map_err(FixedControlError::Resource)?;
    let epoch = super::maintenance_admission::prepare_scoped_invalidation(domain, memory)?;
    let reference = epoch.root_facts();
    let root = RootedDirectory::open_readonly_root(domain.artifact_root(), reference.0)?;
    let binding_file = match root.open_readonly_file(BINDING) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let binding = c::PublisherBinding::decode(&image::<{ c::BINDING_BYTES }>(&binding_file)?)
        .map_err(|_| fixed_invalid())?;
    let transport = Transport::prepare(
        &root,
        reference,
        binding.stream.ok_or_else(fixed_invalid)?,
        memory,
    )?;
    let RemovalState::Removing(spec) = transport.state else {
        return Ok(None);
    };
    let parent = domain
        .workspace_registry(spec.kind, state_root)
        .ok_or_else(fixed_invalid)?;
    if <[u8; 32]>::from(Sha256::digest(parent.as_os_str().as_encoded_bytes())) != spec.parent_path {
        return Err(fixed_invalid());
    }
    let name =
        std::str::from_utf8(&spec.name[..spec.name_len as usize]).map_err(|_| fixed_invalid())?;
    let path = parent.join(name);
    drop(transport);
    drop(epoch);
    drop(root);
    drop(reservation);
    Ok(Some(path))
}

#[cfg(test)]
mod preparation_layout_tests {
    use super::*;
    #[test]
    fn workspace_removal_keeps_large_images_out_of_fixed_owner_layout() {
        // This absolute envelope bound rejects the original large inline owner.
        // Relative sizes of the independent protocol images are not a contract.
        assert!(std::mem::size_of::<PreparedWorkspaceRemoval>() <= 4096);
        let _: fn(PreparedWorkspaceRemoval) = |prepared| {
            let _: MemoryBox<super::super::maintenance_admission::PreparedScopedInvalidation> =
                prepared.epoch;
            let _: MemoryBox<locks::FixedControlScratch> = prepared.scratch;
            let _: MemoryBox<Transport> = prepared.transport;
        };
    }
}
