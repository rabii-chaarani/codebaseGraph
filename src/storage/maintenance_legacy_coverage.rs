//! One control-image step of an explicitly enrolled old reference domain.
//! Preparation owns codecs. The guarded callback only checks and writes bytes.
use super::*;
use crate::storage::locks::{
    self, FixedControlError as Error, FixedControlKind as Kind, FixedControlProblem as Problem,
    FixedFileStamp, RootedDirectory,
};
use crate::storage::maintenance_budget::{
    CleanupMemory, MemoryBox, MemoryReservation, MutationGrant, ReservedBytes,
};
use crate::storage::maintenance_controls as c;

const LEDGER: &[u8] = b".legacy-coverage";
const BYTES: usize = 1024;
const COUNT: usize = 20;
const NAMES: [&[u8]; COUNT] = [
    c::DATA_NAMES[0],
    c::DATA_NAMES[1],
    c::SELECTOR_NAME,
    c::STREAM_NAME,
    c::BINDING_NAME,
    b".use-cohort.json",
    b".workspace-startup.lock",
    b".workspace-removal.data0",
    b".workspace-removal.data1",
    b".workspace-removal.selector",
    b".workspace-removal.binding",
    b".generation-startup.lock",
    b".generation-startup.data0",
    b".generation-startup.data1",
    b".generation-startup.selector",
    b".generation-startup.binding",
    b".reference-activity.lock",
    b".reference-epoch.lock",
    b"worker.lock",
    b".reference-epoch",
];
fn destination(index: usize) -> bool {
    matches!(index, 6 | 11..=15 | 18)
}
fn bad() -> Error {
    Error::Invalid(Problem::Changed)
}
fn native(error: NativeError) -> Error {
    match error {
        NativeError::Io(e) => e.into(),
        _ => bad(),
    }
}
fn put(b: &mut [u8], id: FileIdentity) {
    let (a, c) = id.fixed_words();
    b[..8].copy_from_slice(&a.to_le_bytes());
    b[8..16].copy_from_slice(&c.to_le_bytes());
}
fn get(b: &[u8]) -> FileIdentity {
    FileIdentity::from_fixed_words(
        u64::from_le_bytes(b[..8].try_into().unwrap()),
        u64::from_le_bytes(b[8..16].try_into().unwrap()),
    )
}
#[derive(Clone, Copy)]
struct Ledger {
    managed: FileIdentity,
    artifact: FileIdentity,
    writer: FileIdentity,
    coordination: FileIdentity,
    nonce: [u8; 32],
    operation: [u8; 32],
    scope: [u8; 32],
    index: usize,
    ids: [FileIdentity; COUNT],
}
impl Ledger {
    fn encode(self) -> [u8; BYTES] {
        let mut b = [0; BYTES];
        b[..8].copy_from_slice(b"CGLCOV01");
        b[8] = self.index as u8;
        for (off, id) in [
            (16, self.managed),
            (32, self.artifact),
            (48, self.writer),
            (64, self.coordination),
        ] {
            put(&mut b[off..off + 16], id);
        }
        b[80..112].copy_from_slice(&self.nonce);
        b[112..144].copy_from_slice(&self.operation);
        b[144..176].copy_from_slice(&self.scope);
        for (i, id) in self.ids.into_iter().enumerate() {
            put(&mut b[176 + i * 16..192 + i * 16], id);
        }
        let digest = Sha256::digest(&b[..BYTES - 32]);
        b[BYTES - 32..].copy_from_slice(&digest);
        b
    }
    fn decode(b: &[u8; BYTES]) -> Result<Self, Error> {
        if &b[..8] != b"CGLCOV01"
            || b[8] as usize > COUNT
            || b[9..16].iter().chain(&b[496..BYTES - 32]).any(|x| *x != 0)
            || Sha256::digest(&b[..BYTES - 32]).as_slice() != &b[BYTES - 32..]
        {
            return Err(bad());
        }
        Ok(Self {
            managed: get(&b[16..32]),
            artifact: get(&b[32..48]),
            writer: get(&b[48..64]),
            coordination: get(&b[64..80]),
            nonce: b[80..112].try_into().unwrap(),
            operation: b[112..144].try_into().unwrap(),
            scope: b[144..176].try_into().unwrap(),
            index: b[8] as usize,
            ids: std::array::from_fn(|i| get(&b[176 + i * 16..192 + i * 16])),
        })
    }
}
enum Step {
    Seed,
    Create,
    Fill(FixedFileStamp),
    Acknowledge(FixedFileStamp),
    Done(FixedFileStamp),
}
pub(crate) struct PreparedLegacyCoverage {
    ledger: Ledger,
    expected: Option<([u8; BYTES], FixedFileStamp)>,
    next: [u8; BYTES],
    image: ReservedBytes,
    len: usize,
    step: Step,
    _floor: MemoryReservation,
}
fn bounded_image(file: &std::fs::File, buffer: &mut [u8]) -> Result<usize, Error> {
    let stamp = FixedFileStamp::capture(file, Kind::File)?;
    let len = usize::try_from(stamp.len()).map_err(|_| bad())?;
    if len > buffer.len() {
        return Err(bad());
    }
    locks::read_fixed_range(file, 0, &mut buffer[..len])?;
    stamp.recheck(file)?;
    Ok(len)
}
fn object_image(
    domain: &ReferenceDomain,
    ledger: Ledger,
    own: FileIdentity,
    index: usize,
    out: &mut [u8],
) -> Result<usize, Error> {
    let nonce = ledger.nonce;
    let slot = |out: &mut [u8], initial: &[u8]| {
        c::encode_slot(&mut out[..c::SLOT_BYTES], &nonce, 0, initial)
            .map_err(|_| bad())
            .map(|_| c::SLOT_BYTES)
    };
    let selector = |out: &mut [u8], initial: &[u8]| {
        let mut first = [0; c::SLOT_BYTES];
        let fact = c::encode_slot(&mut first, &nonce, 0, initial).map_err(|_| bad())?;
        out[..c::SELECTOR_BYTES].fill(0);
        c::encode_commit(
            &mut out[..c::COMMIT_BYTES],
            &nonce,
            c::CommitFact {
                generation: 0,
                active: 0,
                slot_digest: fact.digest,
            },
        )
        .map_err(|_| bad())?;
        Ok(c::SELECTOR_BYTES)
    };
    let publication =
        |out: &mut [u8], data: [FileIdentity; 2], selector: FileIdentity, stream: FileIdentity| {
            c::PublisherBinding {
                root: ledger.artifact,
                coordination: ledger.coordination,
                data,
                selector,
                stream: Some(stream),
                incarnation: nonce,
            }
            .encode(&mut out[..c::BINDING_BYTES])
            .map_err(|_| bad())?;
            Ok(c::BINDING_BYTES)
        };
    match index {
        0 => slot(out, b"CGBUILD1"),
        1 => slot(out, b""),
        2 => selector(out, b"CGBUILD1"),
        3 => Ok(0),
        4 => publication(
            out,
            [ledger.ids[0], ledger.ids[1]],
            ledger.ids[2],
            ledger.ids[3],
        ),
        5 => {
            let record = ArtifactCohortRecord {
                version: 2,
                root: ledger.artifact,
                coordination: ledger.coordination,
                coordination_incarnation: nonce,
                origin: CohortOrigin::ControlledMigration,
            };
            let bytes = serde_json::to_vec(&record).map_err(|_| bad())?;
            out[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        6 | 11 => {
            let bytes = locks::encode_coordination_incarnation(nonce);
            out[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        7 => slot(
            out,
            &crate::storage::maintenance_workspace_gate::legacy_bootstrap_idle_image(),
        ),
        8 | 13 => slot(out, b""),
        9 => selector(
            out,
            &crate::storage::maintenance_workspace_gate::legacy_bootstrap_idle_image(),
        ),
        10 => publication(
            out,
            [ledger.ids[7], ledger.ids[8]],
            ledger.ids[9],
            ledger.ids[6],
        ),
        12 => slot(
            out,
            &crate::storage::maintenance_generation_gate::legacy_bootstrap_idle_image(),
        ),
        14 => selector(
            out,
            &crate::storage::maintenance_generation_gate::legacy_bootstrap_idle_image(),
        ),
        15 => {
            use crate::storage::maintenance_generation_gate::{GateBinding, GenerationOriginScope};
            let generations = locks::path_identity(&domain.destination_root.join("generations"))
                .map_err(native)?;
            let bytes = GateBinding {
                managed: ledger.managed,
                generations,
                writer: ledger.writer,
                gate: ledger.ids[11],
                binding: own,
                data: [ledger.ids[12], ledger.ids[13]],
                selector: ledger.ids[14],
                parent: GenerationOriginScope {
                    root: ledger.artifact,
                    coordination: ledger.coordination,
                    cohort: ledger.ids[5],
                    publication_binding: ledger.ids[4],
                    incarnation: nonce,
                },
                nonce,
            }
            .encode();
            out[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        16..=18 => Ok(0),
        19 => {
            let mut incarnation = [0; 32]; // Stable per enrolled bootstrap, not a new epoch on replay.
            incarnation.copy_from_slice(&Sha256::digest(ledger.operation));
            let bytes = encode_reference_epoch(&ReferenceEpochRecord {
                version: 2,
                root: ledger.artifact,
                use_lock: ledger.coordination,
                use_incarnation: nonce,
                cohort: ledger.ids[5],
                scope: domain.scope.clone(),
                incarnation,
                activity: ledger.ids[16],
                serial: ledger.ids[17],
                counter: 0,
                covered: true,
            })
            .map_err(native)?;
            out[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        }
        _ => Err(bad()),
    }
}
/// Requires a preceding genuine administrative enrollment to have initialized
/// the stable root-use header. Merely observing an old root cannot write it.
pub(crate) fn prepare_legacy_coverage(
    domain: &ReferenceDomain,
    memory: &CleanupMemory,
) -> Result<MemoryBox<PreparedLegacyCoverage>, Error> {
    let floor = memory
        .reserve(
            std::mem::size_of::<PreparedLegacyCoverage>()
                + 65536
                + domain.artifact_root.as_os_str().as_encoded_bytes().len() * 24,
        )
        .map_err(Error::Resource)?;
    if domain.scope.kind != 1 || !domain.revalidate().map_err(native)? {
        return Err(bad());
    }
    let root = RootedDirectory::open_readonly_root(
        &domain.artifact_root,
        domain.artifact_identity.ok_or_else(bad)?,
    )?;
    let managed =
        RootedDirectory::open_readonly_root(&domain.destination_root, domain.scope.destination)?;
    let use_file = root.open_readonly_file(b".use.lock")?;
    let coordination = locks::file_identity(&use_file).map_err(native)?;
    let nonce = locks::read_coordination_incarnation(&use_file)
        .map_err(native)?
        .ok_or_else(bad)?;
    let scope: [u8; 32] =
        Sha256::digest(serde_json::to_vec(&domain.scope).map_err(|_| bad())?).into();
    let mut read = [0; 4096];
    let (mut ledger, expected, seed) = match root.open_readonly_file(LEDGER) {
        Ok(file) => {
            let stamp = FixedFileStamp::capture(&file, Kind::File)?;
            if bounded_image(&file, &mut read)? != BYTES {
                return Err(bad());
            }
            let bytes: [u8; BYTES] = read[..BYTES].try_into().unwrap();
            let ledger = Ledger::decode(&bytes)?;
            if locks::created_file_nonce(&file)? != Some(ledger.operation) {
                return Err(bad());
            }
            (ledger, Some((bytes, stamp)), false)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut operation = [0; 32];
            getrandom::fill(&mut operation).map_err(|_| bad())?;
            (
                Ledger {
                    managed: domain.scope.destination,
                    artifact: root.identity(),
                    writer: domain.scope.writer,
                    coordination,
                    nonce,
                    operation,
                    scope,
                    index: 0,
                    ids: [FileIdentity::from_fixed_words(0, 0); COUNT],
                },
                None,
                true,
            )
        }
        Err(e) => return Err(e.into()),
    };
    if ledger.managed != managed.identity()
        || ledger.artifact != root.identity()
        || ledger.writer != domain.scope.writer
        || ledger.coordination != coordination
        || ledger.nonce != nonce
        || ledger.scope != scope
    {
        return Err(bad());
    }
    for (i, control_name) in NAMES.iter().enumerate().take(ledger.index) {
        let parent = if destination(i) { &managed } else { &root };
        let file = parent.open_readonly_file(control_name)?;
        if locks::file_identity(&file).map_err(native)? != ledger.ids[i]
            || locks::created_file_nonce(&file)? != Some(ledger.operation)
        {
            return Err(bad());
        }
    }
    let mut image = memory
        .reserve(4096)
        .map_err(Error::Resource)?
        .bytes(4096)
        .map_err(Error::Resource)?;
    let mut len = 0;
    let mut next = ledger.encode();
    let step = if seed {
        Step::Seed
    } else if ledger.index == COUNT {
        let file = root.open_readonly_file(NAMES[19])?;
        let epoch = read_reference_epoch(&domain.artifact_root).map_err(native)?;
        if !epoch.record.covered
            || epoch.record.scope != domain.scope
            || !epoch_root_matches(&domain.artifact_root, &epoch.record, None).map_err(native)?
        {
            return Err(bad());
        }
        Step::Done(FixedFileStamp::capture(&file, Kind::File)?)
    } else {
        let i = ledger.index;
        let parent = if destination(i) { &managed } else { &root };
        match parent.open_readonly_file(NAMES[i]) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Step::Create,
            Err(e) => return Err(e.into()),
            Ok(file) => {
                let stamp = FixedFileStamp::capture(&file, Kind::File)?;
                if locks::created_file_nonce(&file)? != Some(ledger.operation) {
                    return Err(bad());
                }
                len = object_image(domain, ledger, stamp.identity(), i, image.as_mut_slice())?;
                let found = bounded_image(&file, &mut read)?;
                if found > len || read[..found] != image.as_slice()[..found] {
                    return Err(bad());
                }
                if found != len {
                    Step::Fill(stamp)
                } else {
                    ledger.ids[i] = stamp.identity();
                    ledger.index += 1;
                    next = ledger.encode();
                    Step::Acknowledge(stamp)
                }
            }
        }
    };
    MemoryBox::new(
        PreparedLegacyCoverage {
            ledger,
            expected,
            next,
            image,
            len,
            step,
            _floor: floor,
        },
        memory,
    )
    .map_err(Error::Resource)
}
impl ReferenceDomain {
    /// Recognize only the genuine, still-unactivated generation-gate bootstrap.
    /// This is read continuity, never creation, retirement or recovery authority.
    pub(crate) fn staged_generation_gate(&self, managed: &RootedDirectory) -> Result<bool, Error> {
        if self.scope.kind != 1 || !self.revalidate().map_err(native)? {
            return Err(bad());
        }
        let root = RootedDirectory::open_readonly_root(
            &self.artifact_root,
            self.artifact_identity.ok_or_else(bad)?,
        )?;
        let ledger_file = match root.open_readonly_file(LEDGER) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let ledger_stamp = FixedFileStamp::capture(&ledger_file, Kind::File)?;
        let mut read = [0; 4096];
        if bounded_image(&ledger_file, &mut read)? != BYTES {
            return Err(bad());
        }
        let ledger = Ledger::decode(read[..BYTES].try_into().unwrap())?;
        if !(11..=15).contains(&ledger.index) {
            return Ok(false);
        }
        let use_file = root.open_readonly_file(b".use.lock")?;
        let scope: [u8; 32] =
            Sha256::digest(serde_json::to_vec(&self.scope).map_err(|_| bad())?).into();
        if ledger.managed != managed.identity()
            || ledger.managed != self.scope.destination
            || ledger.artifact != root.identity()
            || ledger.writer != self.scope.writer
            || ledger.coordination != locks::file_identity(&use_file).map_err(native)?
            || locks::read_coordination_incarnation(&use_file).map_err(native)?
                != Some(ledger.nonce)
            || ledger.scope != scope
            || ledger.operation.iter().all(|byte| *byte == 0)
            || locks::created_file_nonce(&ledger_file)? != Some(ledger.operation)
        {
            return Err(bad());
        }
        // Every acknowledged object must retain its actual producer birth.
        for (i, control_name) in NAMES.iter().enumerate().take(ledger.index) {
            let parent = if destination(i) { managed } else { &root };
            let file = parent.open_readonly_file(control_name)?;
            if locks::file_identity(&file).map_err(native)? != ledger.ids[i]
                || locks::created_file_nonce(&file)? != Some(ledger.operation)
            {
                return Err(bad());
            }
        }
        let mut expected = [0; 4096];
        for (i, control_name) in NAMES.iter().enumerate().take(16).skip(11) {
            let file = match managed.open_readonly_file(control_name) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && i >= ledger.index => {
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let stamp = FixedFileStamp::capture(&file, Kind::File)?;
            if i > ledger.index
                || locks::created_file_nonce(&file)? != Some(ledger.operation)
                || (i < ledger.index && stamp.identity() != ledger.ids[i])
            {
                return Err(bad());
            }
            let len = object_image(self, ledger, stamp.identity(), i, &mut expected)?;
            let found = bounded_image(&file, &mut read)?;
            if found > len
                || read[..found] != expected[..found]
                || (i < ledger.index && found != len)
            {
                return Err(bad());
            }
            // The complete binding is activation, even before ledger acknowledgement.
            if i == 15 && found == len {
                return Ok(false);
            }
            stamp.recheck(&file)?;
        }
        ledger_stamp.recheck(&ledger_file)?;
        if !self.revalidate().map_err(native)? {
            return Err(bad());
        }
        Ok(true)
    }
}
impl PreparedLegacyCoverage {
    pub(crate) fn advance(
        &self,
        authority: &impl AdministrativeCoverageAuthority,
        grant: &mut MutationGrant,
    ) -> Result<bool, Error> {
        grant.check()?;
        if !authority.is_administrative()
            || authority.directory().identity() != self.ledger.managed
            || authority.artifact_directory().identity() != self.ledger.artifact
            || authority.writer_identity()? != self.ledger.writer
            || authority.artifact_use_identity()? != self.ledger.coordination
            || authority.proposed_artifact_incarnation() != self.ledger.nonce
        {
            return Err(bad());
        }
        let root = authority.artifact_directory();
        let managed = authority.directory();
        for (i, control_name) in NAMES.iter().enumerate().take(self.ledger.index) {
            grant.take_entry()?;
            let parent = if destination(i) { managed } else { root };
            let file = parent.open_readonly_file(control_name)?;
            if locks::file_identity(&file).map_err(native)? != self.ledger.ids[i]
                || locks::created_file_nonce(&file)? != Some(self.ledger.operation)
            {
                return Err(bad());
            }
        }
        let mut read = [0; BYTES];
        let ledger_file = if let Some((bytes, stamp)) = &self.expected {
            grant.take_entry()?;
            let file = root.open_existing(LEDGER)?;
            stamp.recheck(&file)?;
            locks::read_fixed_range(&file, 0, &mut read)?;
            if &read != bytes {
                return Err(bad());
            }
            Some(file)
        } else {
            None
        };
        match self.step {
            Step::Seed => {
                grant.take_entry()?;
                grant.check_time()?;
                let file = root.create_new(LEDGER)?;
                grant.check_time()?;
                locks::set_created_file_nonce(&file, &self.ledger.operation)?;
                locks::write_fixed_range(&file, 0, &self.next, grant)?;
                grant.check_time()?;
                file.sync_all()?;
                grant.check_time()?;
                root.sync_namespace()?;
            }
            Step::Create => {
                let i = self.ledger.index;
                let parent = if destination(i) { managed } else { root };
                grant.take_entry()?;
                grant.check_time()?;
                let file = parent.create_new(NAMES[i])?;
                grant.check_time()?;
                locks::set_created_file_nonce(&file, &self.ledger.operation)?;
                grant.check_time()?;
                file.sync_all()?;
                grant.check_time()?;
                parent.sync_namespace()?;
            }
            Step::Fill(stamp) => {
                let i = self.ledger.index;
                let parent = if destination(i) { managed } else { root };
                grant.take_entry()?;
                let file = parent.open_existing(NAMES[i])?;
                stamp.recheck(&file)?;
                if locks::created_file_nonce(&file)? != Some(self.ledger.operation) {
                    return Err(bad());
                }
                grant.check_time()?;
                locks::write_fixed_range(&file, 0, &self.image.as_slice()[..self.len], grant)?;
                grant.check_time()?;
                file.sync_all()?;
                grant.check_time()?;
                parent.sync_namespace()?;
            }
            Step::Acknowledge(stamp) => {
                let i = self.ledger.index - 1;
                let parent = if destination(i) { managed } else { root };
                grant.take_entry()?;
                let file = parent.open_readonly_file(NAMES[i])?;
                stamp.recheck(&file)?;
                drop(file);
                let file = ledger_file.ok_or_else(bad)?;
                grant.check_time()?;
                locks::write_fixed_range(&file, 0, &self.next, grant)?;
                grant.check_time()?;
                file.sync_all()?;
            }
            Step::Done(stamp) => {
                grant.take_entry()?;
                let file = root.open_readonly_file(NAMES[19])?;
                stamp.recheck(&file)?;
                return Ok(true);
            }
        }
        grant.check_time()?;
        Ok(false)
    }
}

#[cfg(test)]
mod staged_gate_tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn staged_generation_gate_authenticates_each_restart_prefix_until_activation() {
        let mut operation = [0; 32];
        getrandom::fill(&mut operation).unwrap();
        let suffix = operation
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let fixture = Fixture(std::env::temp_dir().join(format!("cgraph-gate-stage-{suffix}")));
        let layout = ManagedLayout::new(&fixture.0);
        // Build an old, unenrolled namespace. ensure_roots() is a producer
        // factory and would already create the fresh artifact coordination.
        for path in [
            layout.storage_root().to_path_buf(),
            layout.generations_root(),
            layout.runs_root(),
            layout.artifacts_root(),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(layout.writer_lock_path())
            .unwrap();
        let managed =
            RootedDirectory::open(&fixture.0, path_identity(&fixture.0).unwrap()).unwrap();
        let root = managed.open_directory(b"artifacts").unwrap();
        let nonce = [42; 32];
        let mut use_file = root.create_new(b".use.lock").unwrap();
        use_file
            .write_all(&locks::encode_coordination_incarnation(nonce))
            .unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        let mut ledger = Ledger {
            managed: managed.identity(),
            artifact: root.identity(),
            writer: domain.scope.writer,
            coordination: locks::file_identity(&use_file).unwrap(),
            nonce,
            operation,
            scope: Sha256::digest(serde_json::to_vec(&domain.scope).unwrap()).into(),
            index: 0,
            ids: [FileIdentity::from_fixed_words(0, 0); COUNT],
        };
        let mut ledger_file = root.create_new(LEDGER).unwrap();
        locks::set_created_file_nonce(&ledger_file, &operation).unwrap();
        let persist = |file: &mut std::fs::File, ledger: Ledger| {
            file.seek(SeekFrom::Start(0)).unwrap();
            file.write_all(&ledger.encode()).unwrap();
            file.sync_all().unwrap();
        };
        let mut bytes = [0; 4096];
        for (i, control_name) in NAMES.iter().enumerate().take(16) {
            let parent = if destination(i) { &managed } else { &root };
            let mut file = parent.create_new(control_name).unwrap();
            locks::set_created_file_nonce(&file, &operation).unwrap();
            let own = locks::file_identity(&file).unwrap();
            let len = object_image(&domain, ledger, own, i, &mut bytes).unwrap();
            if i >= 11 {
                persist(&mut ledger_file, ledger);
                assert!(
                    domain.staged_generation_gate(&managed).unwrap(),
                    "empty {i}"
                );
                let split = len / 2;
                file.write_all(&bytes[..split]).unwrap();
                assert!(
                    domain.staged_generation_gate(&managed).unwrap(),
                    "prefix {i}"
                );
                file.seek(SeekFrom::Start(0)).unwrap();
                file.write_all(b"!").unwrap();
                assert!(
                    domain.staged_generation_gate(&managed).is_err(),
                    "corrupt {i}"
                );
                file.seek(SeekFrom::Start(0)).unwrap();
            }
            file.write_all(&bytes[..len]).unwrap();
            if i >= 11 {
                assert_eq!(
                    domain.staged_generation_gate(&managed).unwrap(),
                    i != 15,
                    "full image {i}"
                );
            }
            ledger.ids[i] = own;
            ledger.index += 1;
            persist(&mut ledger_file, ledger);
        }
        assert!(!domain.staged_generation_gate(&managed).unwrap());
    }
}
