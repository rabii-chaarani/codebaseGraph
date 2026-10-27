use crate::error::NativeError;
use crate::partition_builder::GraphPartition;
use crate::protocol::{
    LanguageProfile, NativeSyntaxMaterializationRequest, OntologySchema, SourceSnapshot,
    PROFILE_COMPATIBILITY_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const ARTIFACT_FORMAT_VERSION: u64 = 2;
const ENVELOPE_FILE_NAME: &str = "partition.json";
const ARTIFACT_KEY_BYTES: usize = 32;
const ARTIFACT_KEY_HEX_LEN: usize = ARTIFACT_KEY_BYTES * 2;
const ARTIFACT_IO_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArtifactWriteOutcome {
    Written,
    Reused,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ArtifactKeyInput<'a> {
    pub(crate) source_root: &'a str,
    pub(crate) repository_label: &'a str,
    pub(crate) relative_path: &'a str,
    pub(crate) content_hash: &'a str,
    pub(crate) language: &'a str,
    pub(crate) parser_version: &'a str,
    pub(crate) profile: Option<&'a LanguageProfile>,
    pub(crate) ontology: &'a str,
    pub(crate) ontology_schema: &'a OntologySchema,
}

#[derive(Debug, Clone)]
pub(crate) struct ArtifactExpectations<'a> {
    pub(crate) path: &'a str,
    pub(crate) content_hash: &'a str,
    pub(crate) language: &'a str,
}

#[derive(Debug, Clone)]
pub(crate) struct ArtifactStore {
    root: PathBuf,
    // Parser workers share a store. Serialize their short publications so
    // ordinary parallel parsing does not discard optional creation receipts.
    publication: std::sync::Arc<std::sync::Mutex<()>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArtifactEnvelope {
    format_version: u64,
    artifact_key: String,
    partition_payload_sha256: String,
    partition: GraphPartition,
}

#[derive(Serialize)]
struct ArtifactEnvelopeRef<'a> {
    format_version: u64,
    artifact_key: &'a str,
    partition_payload_sha256: &'a str,
    partition: &'a GraphPartition,
}

#[derive(Default)]
struct DigestWriter {
    hasher: Sha256,
}

impl Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl DigestWriter {
    fn finish(self) -> String {
        hex_lower(self.hasher.finalize().as_ref())
    }
}

#[derive(Debug, Clone, Serialize)]
struct ArtifactKeyMaterial<'a> {
    artifact_format_version: u64,
    profile_compatibility_version: u64,
    source_root: &'a str,
    repository_label: &'a str,
    relative_path: &'a str,
    content_hash: &'a str,
    language: &'a str,
    parser_version: &'a str,
    profile: Option<&'a LanguageProfile>,
    ontology: &'a str,
    ontology_schema: &'a OntologySchema,
}

impl ArtifactStore {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            publication: std::sync::Arc::new(std::sync::Mutex::new(())),
        }
    }

    pub(crate) fn key_for_request(
        request: &NativeSyntaxMaterializationRequest,
        snapshot: &SourceSnapshot,
        profile: Option<&LanguageProfile>,
    ) -> Result<String, NativeError> {
        let language = snapshot.language.as_deref().unwrap_or_default();
        artifact_key(&ArtifactKeyInput {
            source_root: &request.source_root,
            repository_label: &request.repository_label,
            relative_path: &snapshot.path,
            content_hash: &snapshot.content_hash,
            language,
            parser_version: &request.parser_version,
            profile,
            ontology: &request.ontology,
            ontology_schema: &request.ontology_schema,
        })
    }

    pub(crate) fn store_partition(
        &self,
        artifact_key: &str,
        partition: &GraphPartition,
    ) -> Result<ArtifactWriteOutcome, NativeError> {
        validate_artifact_key(artifact_key)?;
        let _publication = self
            .publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Hold actual root/use and reference activity through every producer
        // write, publication, equality check and exact-created-leaf cleanup.
        let mutation =
            crate::storage::maintenance_ownership::ReferenceMutation::begin_producing(&self.root)?;
        partition
            .validate_raw_rows()
            .map_err(NativeError::InvalidInput)?;
        if partition.entry.artifact_key.as_deref() != Some(artifact_key) {
            return Err(NativeError::InvalidInput(format!(
                "partition artifact key does not match store key for {}",
                partition.entry.path
            )));
        }
        use crate::storage::locks::{file_identity, path_identity, RootedDirectory};
        use crate::storage::maintenance_controls::try_begin_root_publication;
        use crate::storage::maintenance_origin::{OriginWriter, ORIGIN_NAME};
        let root = RootedDirectory::open(&self.root, path_identity(&self.root)?)?;
        let mut capability = mutation.artifact_root_producer_capability().unwrap_or(None);
        // Tracking is optional. Primary no-replace/equality semantics and actual
        // anchored transport are identical when controls are missing or busy.
        let publication = match capability.as_mut() {
            Some(cap) => try_begin_root_publication(cap).unwrap_or(None),
            None => None,
        };
        let prefix_name = &artifact_key.as_bytes()[..2];
        let prefix = match root.create_directory(prefix_name) {
            Ok(dir) => {
                root.sync_namespace()?;
                dir
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                root.open_directory(prefix_name)?
            }
            Err(error) => return Err(error.into()),
        };
        let key_bytes = decode_key(artifact_key);
        let partition_digest = partition_payload_sha256(partition)?;
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce)
            .map_err(|_| NativeError::InvalidInput("artifact_creation_nonce_unavailable".into()))?;
        let temp_name = format!(".tmp-{artifact_key}-{}", hex_lower(&nonce));
        let temp = root.create_directory(temp_name.as_bytes())?;
        let mut payload = match temp.create_new(ENVELOPE_FILE_NAME.as_bytes()) {
            Ok(file) => file,
            Err(error) => {
                let _ = root.remove_empty_directory(temp_name.as_bytes(), temp.identity());
                return Err(error.into());
            }
        };
        let payload_identity = file_identity(&payload)?;
        #[cfg(test)]
        producer_hook("created", &temp);
        let mut origin = publication.as_ref().and_then(|lease| {
            OriginWriter::create(lease, &temp, &payload, key_bytes, true, true).ok()
        });
        let receipt_identity = origin.as_ref().map(OriginWriter::receipt_identity);
        if let Some(origin) = origin.as_mut() {
            let _ = origin.writing();
        }
        let prepared = (|| -> Result<(u64, [u8; 32]), NativeError> {
            let bytes = write_envelope_file_and_sync(
                &mut payload,
                artifact_key,
                &partition_digest,
                partition,
            )?;
            temp.sync_namespace()?;
            root.sync_namespace()?;
            if let Some(origin) = origin.as_mut() {
                let _ = origin.complete(&temp, &payload, bytes.0, bytes.1);
            }
            Ok(bytes)
        })();
        #[cfg(test)]
        producer_hook("prepared", &temp);
        let accepted = match prepared {
            Ok(accepted) => accepted,
            Err(error) => {
                cleanup_created(
                    &root,
                    &temp,
                    temp_name.as_bytes(),
                    payload_identity,
                    receipt_identity,
                );
                return Err(error);
            }
        };
        let current = (|| -> Result<(), NativeError> {
            if path_identity(&self.root)? != root.identity()
                || root.open_directory(prefix_name)?.identity() != prefix.identity()
            {
                return Err(NativeError::InvalidInput(
                    "artifact_publication_parent_changed".into(),
                ));
            }
            verify_created_shape(&temp, payload_identity, receipt_identity)
        })();
        if let Err(error) = current {
            cleanup_created(
                &root,
                &temp,
                temp_name.as_bytes(),
                payload_identity,
                receipt_identity,
            );
            return Err(error);
        }
        // Windows cannot rename a directory containing open child files. Keep
        // real root/publication ownership and the pinned directory, while the
        // consumed creator supplies the only possible receipt continuation.
        let origin_publication = origin.take().and_then(OriginWriter::into_publication);
        drop(payload);
        match root.rename_directory_no_replace(
            temp_name.as_bytes(),
            &prefix,
            artifact_key.as_bytes(),
            temp.identity(),
        ) {
            Ok(()) => {
                #[cfg(test)]
                producer_hook("published", &temp);
                if path_identity(&self.root)? != root.identity()
                    || root.open_directory(prefix_name)?.identity() != prefix.identity()
                    || prefix.open_directory(artifact_key.as_bytes())?.identity() != temp.identity()
                {
                    return Err(NativeError::InvalidInput(
                        "artifact_published_incarnation_changed".into(),
                    ));
                }
                verify_created_shape(&temp, payload_identity, receipt_identity)?;
                // A real publication durability error remains a primary error;
                // optional origin completion never changes an accepted result.
                prefix.sync_namespace()?;
                root.sync_namespace()?;
                if let Some(lease) = publication.as_ref() {
                    if let Some(origin) = origin_publication {
                        let _ = origin.published(lease, &temp);
                    }
                }
                Ok(ArtifactWriteOutcome::Written)
            }
            Err(rename_error) => {
                let existing = (|| -> Result<ArtifactWriteOutcome, NativeError> {
                    let directory = match prefix.open_directory(artifact_key.as_bytes()) {
                        Ok(dir) => dir,
                        Err(_) => return Err(NativeError::Io(rename_error)),
                    };
                    let existing = match directory.open_readonly_file(ENVELOPE_FILE_NAME.as_bytes())
                    {
                        Ok(existing) => existing,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            let Some(lease) = publication.as_ref() else {
                                return Err(error.into());
                            };
                            if !crate::storage::maintenance_origin::observe_published_origin_without_payload(
                                lease, &root, &directory, key_bytes,
                            )? {
                                return Err(error.into());
                            }
                            // Retirement can leave this exact key directory and
                            // its old receipt behind after removing the payload.
                            // Publish only our newly created inode into the absent
                            // leaf; existing provenance and neighbors stay intact.
                            if path_identity(&self.root)? != root.identity()
                                || root.open_directory(prefix_name)?.identity() != prefix.identity()
                                || prefix.open_directory(artifact_key.as_bytes())?.identity()
                                    != directory.identity()
                            {
                                return Err(NativeError::InvalidInput(
                                    "artifact_publication_parent_changed".into(),
                                ));
                            }
                            temp.rename_file_no_replace(
                                ENVELOPE_FILE_NAME.as_bytes(),
                                &directory,
                                ENVELOPE_FILE_NAME.as_bytes(),
                                payload_identity,
                            )?;
                            if path_identity(&self.root)? != root.identity()
                                || root.open_directory(prefix_name)?.identity() != prefix.identity()
                                || prefix.open_directory(artifact_key.as_bytes())?.identity()
                                    != directory.identity()
                                || file_identity(
                                    &directory.open_readonly_file(ENVELOPE_FILE_NAME.as_bytes())?,
                                )? != payload_identity
                            {
                                return Err(NativeError::InvalidInput(
                                    "artifact_published_incarnation_changed".into(),
                                ));
                            }
                            directory.sync_namespace()?;
                            temp.sync_namespace()?;
                            // The temporary receipt binds its original directory.
                            // It cannot attest this existing directory, and any
                            // older receipt cannot attest the new payload inode.
                            return Ok(ArtifactWriteOutcome::Written);
                        }
                        Err(error) => return Err(error.into()),
                    };
                    let payload = temp.open_readonly_file(ENVELOPE_FILE_NAME.as_bytes())?;
                    if file_identity(&payload)? != payload_identity {
                        return Err(NativeError::InvalidInput(
                            "artifact_temporary_payload_changed".into(),
                        ));
                    }
                    if !same_file_bytes(&existing, &payload)? {
                        return Err(NativeError::InvalidInput(
                            "artifact_existing_payload_differs_or_unproven".into(),
                        ));
                    }
                    if prefix.open_directory(artifact_key.as_bytes())?.identity()
                        != directory.identity()
                    {
                        return Err(NativeError::InvalidInput(
                            "artifact_existing_directory_changed".into(),
                        ));
                    }
                    // A genuine fresh byte-equal producer can attest this one
                    // payload, never the existing directory or its neighbors.
                    if let Some(lease) = publication.as_ref() {
                        match directory.open_readonly_file(ORIGIN_NAME) {
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                if let Ok(mut reused) = OriginWriter::create(
                                    lease, &directory, &existing, key_bytes, false, false,
                                ) {
                                    if reused.writing().is_ok()
                                        && reused
                                            .complete(&directory, &existing, accepted.0, accepted.1)
                                            .is_ok()
                                    {
                                        let _ = reused.published(lease, &directory);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(ArtifactWriteOutcome::Reused)
                })();
                cleanup_created(
                    &root,
                    &temp,
                    temp_name.as_bytes(),
                    payload_identity,
                    receipt_identity,
                );
                existing
            }
        }
    }

    pub(crate) fn load_partition(
        &self,
        artifact_key: &str,
        expected: &ArtifactExpectations<'_>,
    ) -> Result<Option<GraphPartition>, NativeError> {
        if validate_artifact_key(artifact_key).is_err() {
            return Ok(None);
        }
        let Some(_use_lease) =
            crate::storage::maintenance_ownership::ArtifactUseLease::try_existing_shared_read(
                &self.root,
            )?
        else {
            return Ok(None);
        };
        use crate::storage::locks::{path_identity, RootedDirectory};
        let root = RootedDirectory::open_readonly_root(&self.root, path_identity(&self.root)?)?;
        let file = match root
            .open_directory_readonly(&artifact_key.as_bytes()[..2])
            .and_then(|prefix| prefix.open_directory_readonly(artifact_key.as_bytes()))
            .and_then(|directory| directory.open_readonly_file(ENVELOPE_FILE_NAME.as_bytes()))
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        load_partition_file(file, artifact_key, expected)
    }

    pub(crate) fn list_keys(&self) -> Result<Vec<String>, NativeError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        ensure_artifact_root(&self.root)?;

        let mut keys = Vec::new();
        for prefix_entry in fs::read_dir(&self.root)? {
            let prefix_entry = prefix_entry?;
            let prefix_type = prefix_entry.file_type()?;
            if prefix_type.is_symlink() {
                return Err(NativeError::InvalidInput(format!(
                    "refusing to inspect symlinked artifact path {}",
                    prefix_entry.path().display()
                )));
            }
            if !prefix_type.is_dir() {
                continue;
            }
            let prefix_name = prefix_entry.file_name();
            let prefix = prefix_name.to_string_lossy();
            if prefix.starts_with(".tmp-") || !is_valid_artifact_prefix(&prefix) {
                continue;
            }
            for artifact_entry in fs::read_dir(prefix_entry.path())? {
                let artifact_entry = artifact_entry?;
                let artifact_type = artifact_entry.file_type()?;
                if artifact_type.is_symlink() {
                    return Err(NativeError::InvalidInput(format!(
                        "refusing to inspect symlinked artifact path {}",
                        artifact_entry.path().display()
                    )));
                }
                if !artifact_type.is_dir() {
                    continue;
                }
                let key = artifact_entry.file_name().to_string_lossy().to_string();
                if validate_artifact_key(&key).is_ok() && key.starts_with(prefix.as_ref()) {
                    keys.push(key);
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    #[cfg(test)]
    pub(crate) fn delete_key(&self, artifact_key: &str) -> Result<bool, NativeError> {
        if validate_artifact_key(artifact_key).is_err() {
            return Ok(false);
        }
        Err(NativeError::InvalidInput(
            "artifact_collection_requires_current_maintenance_authority".into(),
        ))
    }
}

fn validate_artifact_key(artifact_key: &str) -> Result<(), NativeError> {
    if artifact_key.len() != ARTIFACT_KEY_HEX_LEN {
        return Err(NativeError::InvalidInput(format!(
            "artifact key must be {ARTIFACT_KEY_HEX_LEN} hex characters"
        )));
    }
    if !artifact_key
        .bytes()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(NativeError::InvalidInput(
            "artifact key must be lowercase hexadecimal".to_string(),
        ));
    }
    Ok(())
}

fn is_valid_artifact_prefix(prefix: &str) -> bool {
    prefix.len() == 2
        && prefix
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn load_partition_file(
    file: File,
    artifact_key: &str,
    expected: &ArtifactExpectations<'_>,
) -> Result<Option<GraphPartition>, NativeError> {
    let file_bytes = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(ARTIFACT_IO_BUFFER_BYTES, file);
    let envelope: ArtifactEnvelope =
        match serde_json::from_reader(&mut reader.by_ref().take(file_bytes)) {
            Ok(envelope) => envelope,
            Err(_) => return Ok(None),
        };
    if reader.get_ref().metadata()?.len() != file_bytes {
        return Ok(None);
    }
    if envelope.format_version != ARTIFACT_FORMAT_VERSION || envelope.artifact_key != artifact_key {
        return Ok(None);
    }
    if envelope.partition.entry.path != expected.path
        || envelope.partition.entry.content_hash != expected.content_hash
        || envelope.partition.entry.language != expected.language
    {
        return Ok(None);
    }
    if envelope.partition.entry.artifact_key.as_deref() != Some(artifact_key) {
        return Ok(None);
    }
    let payload_sha256 = match partition_payload_sha256(&envelope.partition) {
        Ok(payload_sha256) => payload_sha256,
        Err(_) => return Ok(None),
    };
    if envelope.partition_payload_sha256 != payload_sha256 {
        return Ok(None);
    }
    if envelope.partition.validate_raw_rows().is_err() {
        return Ok(None);
    }
    Ok(Some(envelope.partition))
}

// Only this specific primary publication conflict permits a pipeline-local
// rebuild copy. IO, source changes and other guards propagate.
pub(crate) fn is_canonical_payload_conflict(error: &NativeError) -> bool {
    matches!(error, NativeError::InvalidInput(reason)
        if reason == "artifact_existing_payload_differs_or_unproven")
}

/// An actual create_new owner for one per-run rebuild copy. It never adopts a
/// canonical cache entry or grants reference/DB/cleanup authority. Files are
/// closed between planning and staging; only fixed creator facts are retained.
pub(crate) struct TransientPartition {
    path: PathBuf,
    parent: crate::storage::locks::FileIdentity,
    identity: crate::storage::locks::FileIdentity,
    nonce: Option<[u8; 32]>,
    stamp: Option<crate::storage::locks::FixedFileStamp>,
    key: String,
}
impl TransientPartition {
    pub(crate) fn plan_bytes<'a>(staging: &str, paths: impl Iterator<Item = &'a str>) -> u64 {
        // Owner slots, conservative path/key capacities and the extra retained
        // conflict diagnostic are reserved before parse-worker fanout.
        let fixed = (std::mem::size_of::<Option<Self>>() as u64)
            .saturating_add((staging.len() as u64).saturating_add(80).saturating_mul(2))
            .saturating_add(512);
        paths.fold(
            std::mem::size_of::<Vec<Option<Self>>>() as u64,
            |bytes, path| {
                bytes
                    .saturating_add(fixed)
                    .saturating_add((path.len() as u64).saturating_mul(2))
            },
        )
    }
    pub(crate) fn create(
        staging: &std::path::Path,
        key: &str,
        partition: &GraphPartition,
    ) -> Result<Self, NativeError> {
        use crate::process_lifecycle::FreshLifecycleFile;
        use crate::storage::{locks, maintenance_controls::WorkspaceControlFile};
        validate_artifact_key(key)?;
        let parent = locks::path_identity(staging)?;
        let mut operation = [0; 32];
        getrandom::fill(&mut operation).map_err(|_| {
            NativeError::InvalidInput("transient_artifact_nonce_unavailable".into())
        })?;
        let path = staging.join(format!(
            ".partition-fallback-{}.json",
            hex_lower(&operation)
        ));
        let fresh = FreshLifecycleFile::create(&path).map_err(NativeError::InvalidInput)?;
        let mut owner = Self {
            path: path.clone(),
            parent,
            identity: locks::file_identity(fresh.file())?,
            nonce: None,
            stamp: None,
            key: key.into(),
        };
        let mut output = WorkspaceControlFile::from_created(fresh, &path);
        owner.nonce = match &output {
            WorkspaceControlFile::Tracked(tracked) => {
                let _ =
                    crate::storage::maintenance_origin::record_created_workspace_control(tracked);
                Some(tracked.origin_binding().operation)
            }
            WorkspaceControlFile::Plain(fresh) => {
                match locks::set_created_file_nonce(fresh.file(), &operation) {
                    Ok(()) => {
                        if locks::created_file_nonce(fresh.file())? != Some(operation) {
                            return Err(NativeError::InvalidInput(
                                "transient_artifact_creator_changed".into(),
                            ));
                        }
                        Some(operation)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Unsupported => None,
                    Err(error) => return Err(error.into()),
                }
            }
        };
        let digest = partition_payload_sha256(partition)?;
        {
            let mut writer = BufWriter::with_capacity(ARTIFACT_IO_BUFFER_BYTES, &mut output);
            write_envelope(&mut writer, key, &digest, partition)?;
            writer.flush()?;
        }
        if let WorkspaceControlFile::Plain(fresh) = &output {
            owner.stamp = Some(
                locks::FixedFileStamp::capture(fresh.file(), locks::FixedControlKind::File)
                    .map_err(|_| {
                        NativeError::InvalidInput("transient_artifact_creator_changed".into())
                    })?,
            );
        }
        output.finish_created_recorded()?;
        let file = owner.open_created()?;
        owner.stamp = Some(
            locks::FixedFileStamp::capture(&file, locks::FixedControlKind::File).map_err(|_| {
                NativeError::InvalidInput("transient_artifact_creator_changed".into())
            })?,
        );
        Ok(owner)
    }
    fn open_created(&self) -> Result<File, NativeError> {
        use crate::storage::locks::{self, RootedDirectory};
        let parent = RootedDirectory::open_readonly_root(self.path.parent().unwrap(), self.parent)?;
        let file = parent.open_readonly_file(self.path.file_name().unwrap().as_encoded_bytes())?;
        if locks::file_identity(&file)? != self.identity
            || self
                .nonce
                .is_some_and(|nonce| locks::created_file_nonce(&file).ok().flatten() != Some(nonce))
        {
            return Err(NativeError::InvalidInput(
                "transient_artifact_creator_changed".into(),
            ));
        }
        if let Some(stamp) = self.stamp {
            stamp
                .recheck(&file)
                .map_err(|_| NativeError::InvalidInput("transient_artifact_changed".into()))?;
        }
        Ok(file)
    }
    pub(crate) fn load(
        &self,
        expected: &ArtifactExpectations<'_>,
    ) -> Result<GraphPartition, NativeError> {
        let file = self.open_created()?;
        let partition = load_partition_file(file, &self.key, expected)?
            .ok_or_else(|| NativeError::InvalidInput("transient_artifact_invalid".into()))?;
        drop(self.open_created()?);
        Ok(partition)
    }
}
impl Drop for TransientPartition {
    fn drop(&mut self) {
        // Replacements/aliases remain at their own paths. Never recurse or move
        // the parent or a neighbor; only the exact file we exclusively created.
        if (self.nonce.is_none() && self.stamp.is_none()) || self.open_created().is_err() {
            return;
        }
        if let Ok(parent) =
            crate::storage::locks::RootedDirectory::open(self.path.parent().unwrap(), self.parent)
        {
            let _ = parent.unlink_file(
                self.path.file_name().unwrap().as_encoded_bytes(),
                self.identity,
            );
            let _ = parent.sync_namespace();
        }
    }
}

fn write_envelope(
    writer: &mut impl Write,
    artifact_key: &str,
    partition_payload_sha256: &str,
    partition: &GraphPartition,
) -> Result<(), NativeError> {
    serde_json::to_writer_pretty(
        writer,
        &ArtifactEnvelopeRef {
            format_version: ARTIFACT_FORMAT_VERSION,
            artifact_key,
            partition_payload_sha256,
            partition,
        },
    )?;
    Ok(())
}

struct AcceptedWriter<'a> {
    file: &'a mut File,
    hash: Sha256,
    len: u64,
}
impl Write for AcceptedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(bytes)?;
        self.hash.update(&bytes[..n]);
        self.len = self
            .len
            .checked_add(n as u64)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
fn write_envelope_file_and_sync(
    file: &mut File,
    artifact_key: &str,
    partition_payload_sha256: &str,
    partition: &GraphPartition,
) -> Result<(u64, [u8; 32]), NativeError> {
    let accepted = AcceptedWriter {
        file,
        hash: Sha256::new(),
        len: 0,
    };
    let mut writer = BufWriter::with_capacity(ARTIFACT_IO_BUFFER_BYTES, accepted);
    write_envelope(
        &mut writer,
        artifact_key,
        partition_payload_sha256,
        partition,
    )?;
    writer.flush()?;
    writer.get_mut().file.sync_all()?;
    let accepted = writer
        .into_inner()
        .map_err(|error| NativeError::Io(error.into_error()))?;
    Ok((accepted.len, accepted.hash.finalize().into()))
}
fn decode_key(text: &str) -> [u8; 32] {
    let mut key = [0; 32];
    for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        key[i] = (digit(pair[0]) << 4) | digit(pair[1]);
    }
    key
}
fn verify_created_shape(
    directory: &crate::storage::locks::RootedDirectory,
    payload: crate::storage::locks::FileIdentity,
    receipt: Option<crate::storage::locks::FileIdentity>,
) -> Result<(), NativeError> {
    use crate::storage::locks::file_identity;
    let mut bytes = [0; 4096];
    let mut found_payload = false;
    let mut found_receipt = false;
    directory.visit_entries(&mut bytes, &mut |name| {
        let expected = if name == ENVELOPE_FILE_NAME.as_bytes() {
            found_payload = true;
            Some(payload)
        } else if name == crate::storage::maintenance_origin::ORIGIN_NAME {
            found_receipt = true;
            receipt
        } else {
            None
        };
        let Some(expected) = expected else {
            return Err(std::io::ErrorKind::InvalidData.into());
        };
        let file = directory.open_readonly_file(name)?;
        if file_identity(&file)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))?
            != expected
        {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        Ok(())
    })?;
    if !found_payload || found_receipt != receipt.is_some() {
        return Err(NativeError::InvalidInput(
            "artifact_created_leaf_changed".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
type ProducerHook = Box<dyn FnMut(&str, &crate::storage::locks::RootedDirectory)>;
#[cfg(test)]
thread_local! {static PRODUCER_HOOK:std::cell::RefCell<Option<ProducerHook>> =std::cell::RefCell::new(None);}
#[cfg(test)]
fn producer_hook(stage: &str, directory: &crate::storage::locks::RootedDirectory) {
    PRODUCER_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(stage, directory);
        }
    });
}
/// Fixture-only interruption after the real producer has durably prepared its
/// payload and receipt. No test constructs authority or receipt bytes.
#[cfg(test)]
pub(crate) fn leave_prepared_temporary_for_test(
    store: &ArtifactStore,
    key: &str,
    partition: &GraphPartition,
) -> Result<(), NativeError> {
    struct PreparedInterruption;
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            PRODUCER_HOOK.with(|hook| {
                hook.borrow_mut().take();
            });
        }
    }
    PRODUCER_HOOK.with(|hook| {
        assert!(
            hook.borrow().is_none(),
            "producer fixture hooks cannot nest"
        );
        *hook.borrow_mut() = Some(Box::new(|stage, _| {
            if stage == "prepared" {
                std::panic::panic_any(PreparedInterruption);
            }
        }));
    });
    let reset = Reset;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.store_partition(key, partition)
    }));
    drop(reset);
    match result {
        Err(payload) if payload.is::<PreparedInterruption>() => Ok(()),
        Err(payload) => std::panic::resume_unwind(payload),
        Ok(Err(error)) => Err(error),
        Ok(Ok(_)) => Err(NativeError::InvalidInput(
            "producer_preparation_fault_not_reached".into(),
        )),
    }
}
fn cleanup_created(
    root: &crate::storage::locks::RootedDirectory,
    temp: &crate::storage::locks::RootedDirectory,
    name: &[u8],
    payload: crate::storage::locks::FileIdentity,
    receipt: Option<crate::storage::locks::FileIdentity>,
) {
    let _ = temp.unlink_file(ENVELOPE_FILE_NAME.as_bytes(), payload);
    if let Some(receipt) = receipt {
        let _ = temp.unlink_file(crate::storage::maintenance_origin::ORIGIN_NAME, receipt);
    }
    let _ = temp.sync_namespace();
    let _ = root.remove_empty_directory(name, temp.identity());
    let _ = root.sync_namespace();
}
fn same_file_bytes(existing: &File, candidate: &File) -> Result<bool, NativeError> {
    use crate::storage::locks::{FixedControlKind, FixedFileStamp};
    use std::io::{Seek, SeekFrom};
    let invalid =
        || NativeError::InvalidInput("artifact_equality_source_changed_or_aliased".into());
    let old = FixedFileStamp::capture(existing, FixedControlKind::File).map_err(|_| invalid())?;
    let new = FixedFileStamp::capture(candidate, FixedControlKind::File).map_err(|_| invalid())?;
    if old.len() != new.len() {
        return Ok(false);
    }
    let (mut left, mut right) = (existing, candidate);
    left.seek(SeekFrom::Start(0))?;
    right.seek(SeekFrom::Start(0))?;
    let mut a = [0; 8192];
    let mut b = [0; 8192];
    loop {
        let na = left.read(&mut a)?;
        let nb = right.read(&mut b)?;
        if na != nb || a[..na] != b[..nb] {
            return Ok(false);
        }
        if na == 0 {
            break;
        }
    }
    old.recheck(existing).map_err(|_| invalid())?;
    new.recheck(candidate).map_err(|_| invalid())?;
    Ok(true)
}

fn partition_payload_sha256(partition: &GraphPartition) -> Result<String, NativeError> {
    let mut writer = DigestWriter::default();
    serde_json::to_writer(&mut writer, partition)?;
    Ok(writer.finish())
}

fn ensure_artifact_root(root: &std::path::Path) -> Result<(), NativeError> {
    if !root.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(NativeError::InvalidInput(format!(
            "artifact root must be a real directory: {}",
            root.display()
        )));
    }
    Ok(())
}

pub(crate) fn artifact_key(input: &ArtifactKeyInput<'_>) -> Result<String, NativeError> {
    let material = ArtifactKeyMaterial {
        artifact_format_version: ARTIFACT_FORMAT_VERSION,
        profile_compatibility_version: PROFILE_COMPATIBILITY_VERSION,
        source_root: input.source_root,
        repository_label: input.repository_label,
        relative_path: input.relative_path,
        content_hash: input.content_hash,
        language: input.language,
        parser_version: input.parser_version,
        profile: input.profile,
        ontology: input.ontology,
        ontology_schema: input.ontology_schema,
    };
    let encoded = serde_json::to_vec(&material)?;
    let digest = Sha256::digest(encoded);
    Ok(hex_lower(digest.as_ref()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash;
    use crate::partition_builder::GraphPartition;
    use crate::protocol::{CaptureMapping, ManifestEntry, OntologyRelationType, OntologySchema};
    use crate::syntax_materializer::{GraphEdgeRow, GraphNodeRow};
    use serde_json::json;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sample_profile() -> LanguageProfile {
        LanguageProfile {
            language: "rust".to_string(),
            suffixes: vec![".rs".to_string()],
            grammar_package: "tree_sitter_rust".to_string(),
            grammar_version: "tree_sitter_rust@0.24.2".to_string(),
            root_node_types: vec!["source_file".to_string()],
            capture_mappings: vec![CaptureMapping {
                capture_name: "definition.function".to_string(),
                parser_node_types: vec!["function_item".to_string()],
                target_node_type: "Function".to_string(),
                relation_types: vec!["Contains".to_string()],
                context_rule: String::new(),
                construct: String::new(),
            }],
        }
    }

    fn sample_request() -> NativeSyntaxMaterializationRequest {
        NativeSyntaxMaterializationRequest {
            source_root: "/repo".to_string(),
            repository_label: "repo".to_string(),
            mode: "changed".to_string(),
            parser_version: "native-1".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: OntologySchema {
                relation_types: vec![OntologyRelationType {
                    name: "Calls".to_string(),
                    source_types: vec!["Function".to_string()],
                    target_types: vec!["Function".to_string()],
                }],
            },
            previous_manifest: None,
            profiles: vec![sample_profile()],
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            artifact_root: String::new(),
            db_path: "/repo/.codebaseGraph/graph.ldb".to_string(),
            include_fts: true,
            schema_statements: Vec::new(),
            staging_dir: "/repo/.codebaseGraph/staging".to_string(),
            atomic_rebuild: false,
            strict: false,
            parallel: true,
            spill_chunk_mib: 32,
            max_parallelism: 2,
            progress: false,
        }
    }

    fn sample_snapshot() -> SourceSnapshot {
        SourceSnapshot {
            path: "src/lib.rs".to_string(),
            absolute_path: "/repo/src/lib.rs".to_string(),
            content_hash: "content-hash".to_string(),
            language: Some("rust".to_string()),
            byte_len: 0,
            source: None,
        }
    }

    fn sample_partition() -> GraphPartition {
        let nodes = vec![GraphNodeRow {
            id: "node:1".to_string(),
            table: "Function".to_string(),
            label: "main".to_string(),
            kind: "definition.function".to_string(),
            language: "rust".to_string(),
            grammar_version: None,
            path: "src/lib.rs".to_string(),
            qualified_name: "crate::main".to_string(),
            scope_id: "scope:root".to_string(),
            line_start: Some(1),
            line_end: Some(3),
            byte_start: Some(0),
            byte_end: Some(42),
            tree_sitter_node_type: "function_item".to_string(),
            capture_name: "definition.function".to_string(),
            summary: "fn main()".to_string(),
            metadata: json!({"kind":"function"}),
        }];
        let edges = vec![GraphEdgeRow {
            id: "edge:1".to_string(),
            edge_type: "Calls".to_string(),
            source_id: "node:1".to_string(),
            target_id: "node:2".to_string(),
            kind: "reference.call".to_string(),
            confidence: 0.95,
            field_name: None,
            child_index: None,
            line_start: Some(2),
            line_end: Some(2),
            byte_start: Some(10),
            byte_end: Some(18),
            metadata: json!({"kind":"call"}),
        }];
        GraphPartition {
            entry: ManifestEntry {
                path: "src/lib.rs".to_string(),
                content_hash: "content-hash".to_string(),
                language: "rust".to_string(),
                partition_id: hash::partition_id("src/lib.rs"),
                artifact_key: None,
                node_ids: nodes.iter().map(|node| node.id.clone()).collect(),
                edge_ids: edges.iter().map(|edge| edge.id.clone()).collect(),
                node_types: nodes
                    .iter()
                    .map(|node| (node.id.clone(), node.table.clone()))
                    .collect(),
                edge_types: edges
                    .iter()
                    .map(|edge| (edge.id.clone(), edge.edge_type.clone()))
                    .collect(),
                node_count: nodes.len(),
                edge_count: edges.len(),
                materialized_at: "unix:0".to_string(),
            },
            nodes,
            edges,
        }
    }

    fn sample_partition_for_key(artifact_key: &str) -> GraphPartition {
        let mut partition = sample_partition();
        partition.set_artifact_key(artifact_key.to_string());
        partition
    }

    #[test]
    fn artifact_key_changes_when_inputs_change() {
        let request = sample_request();
        let snapshot = sample_snapshot();
        let profile = request.profiles.first().unwrap();
        let baseline = ArtifactStore::key_for_request(&request, &snapshot, Some(profile)).unwrap();

        let mut changed_snapshot = snapshot.clone();
        changed_snapshot.path = "src/renamed.rs".to_string();
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&request, &changed_snapshot, Some(profile)).unwrap()
        );

        let mut changed_snapshot = snapshot.clone();
        changed_snapshot.content_hash = "other-hash".to_string();
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&request, &changed_snapshot, Some(profile)).unwrap()
        );

        let mut changed_snapshot = snapshot.clone();
        changed_snapshot.language = Some("python".to_string());
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&request, &changed_snapshot, Some(profile)).unwrap()
        );

        let mut changed_request = sample_request();
        changed_request.parser_version = "native-2".to_string();
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&changed_request, &snapshot, Some(profile)).unwrap()
        );

        let mut changed_request = sample_request();
        changed_request.ontology = "code_ontology_v2".to_string();
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&changed_request, &snapshot, Some(profile)).unwrap()
        );

        let mut changed_request = sample_request();
        changed_request
            .ontology_schema
            .relation_types
            .push(OntologyRelationType {
                name: "Contains".to_string(),
                source_types: vec!["File".to_string()],
                target_types: vec!["Function".to_string()],
            });
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&changed_request, &snapshot, Some(profile)).unwrap()
        );

        let mut changed_profile = profile.clone();
        changed_profile.capture_mappings.push(CaptureMapping {
            capture_name: "definition.struct".to_string(),
            parser_node_types: vec!["struct_item".to_string()],
            target_node_type: "Class".to_string(),
            relation_types: Vec::new(),
            context_rule: String::new(),
            construct: String::new(),
        });
        assert_ne!(
            baseline,
            ArtifactStore::key_for_request(&request, &snapshot, Some(&changed_profile)).unwrap()
        );
    }

    #[test]
    fn transient_partition_loads_only_actual_created_file_and_preserves_replacement() {
        let root = unique_temp_dir("codebase-graph-transient-partition");
        fs::create_dir_all(&root).unwrap();
        let key = "a".repeat(64);
        let partition = sample_partition_for_key(&key);
        let expected = ArtifactExpectations {
            path: "src/lib.rs",
            content_hash: "content-hash",
            language: "rust",
        };
        let owner = TransientPartition::create(&root, &key, &partition).unwrap();
        let created = owner.path.clone();
        owner.load(&expected).unwrap().validate_raw_rows().unwrap();
        assert!(owner.load(&expected).is_ok());
        drop(owner);
        assert!(!created.exists());
        let owner = TransientPartition::create(&root, &key, &partition).unwrap();
        let replaced = owner.path.clone();
        let retained = root.join("retained-own-copy");
        fs::rename(&replaced, &retained).unwrap();
        fs::write(&replaced, b"foreign replacement must remain").unwrap();
        let id = crate::storage::locks::path_identity(&replaced).unwrap();
        assert!(owner.load(&expected).is_err());
        drop(owner);
        assert_eq!(crate::storage::locks::path_identity(&replaced).unwrap(), id);
        assert_eq!(
            fs::read(&replaced).unwrap(),
            b"foreign replacement must remain"
        );
        assert!(retained.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transient_fallback_selects_only_explicit_canonical_payload_conflict() {
        assert!(is_canonical_payload_conflict(&NativeError::InvalidInput(
            "artifact_existing_payload_differs_or_unproven".into(),
        )));
        assert!(!is_canonical_payload_conflict(&NativeError::InvalidInput(
            "artifact_publication_parent_changed".into(),
        )));
        assert!(!is_canonical_payload_conflict(&NativeError::Io(
            std::io::ErrorKind::NotFound.into()
        )));
    }

    #[test]
    fn artifact_store_roundtrips_raw_partition_and_reuses_existing_payload() {
        let root = unique_temp_dir("codebase-graph-artifacts");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();
        let partition = sample_partition_for_key(&key);

        let first = store.store_partition(&key, &partition).unwrap();
        let second = store.store_partition(&key, &partition).unwrap();
        let loaded = store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust",
                },
            )
            .unwrap()
            .unwrap();

        assert_eq!(first, ArtifactWriteOutcome::Written);
        assert_eq!(second, ArtifactWriteOutcome::Reused);
        assert_eq!(loaded.entry.artifact_key.as_deref(), Some(key.as_str()));
        loaded.validate_raw_rows().unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn streaming_partition_checksum_matches_the_legacy_canonical_payload() {
        let partition = sample_partition();
        let legacy_payload = serde_json::to_vec(&partition).unwrap();
        let legacy_checksum = hex_lower(Sha256::digest(legacy_payload).as_ref());

        assert_eq!(
            partition_payload_sha256(&partition).unwrap(),
            legacy_checksum
        );
    }

    #[test]
    fn artifact_store_treats_corrupt_or_mismatched_payloads_as_cache_miss() {
        let root = unique_temp_dir("codebase-graph-artifacts-corrupt");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();
        let payload_dir = root.join(&key[..2]).join(&key);
        fs::create_dir_all(&payload_dir).unwrap();
        fs::write(payload_dir.join(ENVELOPE_FILE_NAME), "{not-json").unwrap();

        let loaded = store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust",
                },
            )
            .unwrap();
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_store_rejects_prior_format_envelopes() {
        let root = unique_temp_dir("codebase-graph-artifacts-format");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();
        let partition = sample_partition_for_key(&key);
        store.store_partition(&key, &partition).unwrap();

        let payload_path = root.join(&key[..2]).join(&key).join(ENVELOPE_FILE_NAME);
        let mut envelope: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&payload_path).unwrap()).unwrap();
        envelope["format_version"] = serde_json::Value::from(ARTIFACT_FORMAT_VERSION - 1);
        fs::write(&payload_path, serde_json::to_vec(&envelope).unwrap()).unwrap();

        let loaded = store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust",
                },
            )
            .unwrap();
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_store_rejects_wrong_expected_content() {
        let root = unique_temp_dir("codebase-graph-artifacts-expected");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();
        let partition = sample_partition_for_key(&key);
        store.store_partition(&key, &partition).unwrap();

        let loaded = store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "other-hash",
                    language: "rust",
                },
            )
            .unwrap();
        assert!(loaded.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_artifact_keys_are_treated_as_cache_miss_and_safe_delete_noops() {
        let root = unique_temp_dir("codebase-graph-artifacts-malformed");
        let store = ArtifactStore::new(&root);

        assert!(store
            .load_partition(
                "no",
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust",
                }
            )
            .unwrap()
            .is_none());
        assert!(!store.delete_key("not-hex").unwrap());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn artifact_store_commits_only_durable_artifact_directories() {
        let root = unique_temp_dir("codebase-graph-artifacts-durable");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();

        let partition = sample_partition_for_key(&key);
        store.store_partition(&key, &partition).unwrap();

        let keys = store.list_keys().unwrap();
        assert_eq!(keys, vec![key.clone()]);
        assert!(root
            .join(&key[..2])
            .join(&key)
            .join(ENVELOPE_FILE_NAME)
            .exists());
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(".tmp-"))
                .count(),
            0
        );
        assert!(store
            .delete_key(&key)
            .unwrap_err()
            .to_string()
            .contains("requires_current_maintenance_authority"));
        assert_eq!(store.list_keys().unwrap(), vec![key.clone()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_existing_artifact_is_preserved_on_prepublication_failure() {
        let root = unique_temp_dir("codebase-graph-artifacts-replace");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();

        assert_eq!(
            store
                .store_partition(&key, &sample_partition_for_key(&key))
                .unwrap(),
            ArtifactWriteOutcome::Written
        );

        let payload_path = root.join(&key[..2]).join(&key).join(ENVELOPE_FILE_NAME);
        let mut corrupted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&payload_path).unwrap()).unwrap();
        corrupted["partition_payload_sha256"] = serde_json::Value::String("deadbeef".to_string());
        fs::write(
            &payload_path,
            serde_json::to_vec_pretty(&corrupted).unwrap(),
        )
        .unwrap();

        let before = fs::read(&payload_path).unwrap();
        let before_identity = crate::storage::locks::path_identity(&payload_path).unwrap();
        assert!(store
            .store_partition(&key, &sample_partition_for_key(&key))
            .unwrap_err()
            .to_string()
            .contains("differs_or_unproven"));
        assert_eq!(fs::read(&payload_path).unwrap(), before);
        assert_eq!(
            crate::storage::locks::path_identity(&payload_path).unwrap(),
            before_identity
        );
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn artifact_cleanup_rejects_symlinked_entries() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("artifact-store-symlink-cleanup");
        let store = ArtifactStore::new(root.join("artifacts"));
        let key = "c".repeat(64);
        let prefix = root.join("artifacts").join("cc");
        let outside = root.join("outside");
        fs::create_dir_all(&prefix).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        symlink(&outside, prefix.join(&key)).unwrap();

        let error = store.delete_key(&key).unwrap_err();
        assert!(error
            .to_string()
            .contains("requires_current_maintenance_authority"));
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).unwrap(),
            "keep"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dc_s3_artifact_producing_guard_covers_write_reuse_replace_and_failure() {
        use crate::storage::layout::{create_fresh_reference_root, ManagedLayout};
        use crate::storage::locks::{open_locked, LockMode};
        use crate::storage::maintenance_ownership::{
            initialize_fresh_reference_domain, observe_reference_epoch, EpochObservation,
            ReferenceDomain,
        };
        let root = unique_temp_dir("cleanup-s3-real-artifact-epoch");
        fs::create_dir_all(&root).unwrap();
        let layout = ManagedLayout::new(root.join("storage"));
        let fresh = create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        let observe = || match observe_reference_epoch(&domain).unwrap() {
            EpochObservation::Verified(epoch) => epoch,
            other => panic!("producer did not release reference activity: {other:?}"),
        };
        let store = ArtifactStore::new(layout.artifacts_root());
        let key = "a".repeat(64);
        let partition = sample_partition_for_key(&key);
        let before = observe();
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Written
        );
        let written = observe();
        assert!(written.counter() > before.counter());
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Reused
        );
        let reused = observe();
        assert!(reused.counter() > written.counter());
        let payload = layout
            .artifacts_root()
            .join("aa")
            .join(&key)
            .join(ENVELOPE_FILE_NAME);
        fs::write(&payload, b"corrupt old payload").unwrap();
        assert!(store.store_partition(&key, &partition).is_err());
        let rejected = observe();
        assert!(rejected.counter() > reused.counter());
        assert_eq!(fs::read(&payload).unwrap(), b"corrupt old payload");
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_none());
        let blocked_key = "b".repeat(64);
        let blocked_prefix = layout.artifacts_root().join("bb");
        fs::write(&blocked_prefix, b"foreign prefix sentinel").unwrap();
        assert!(store
            .store_partition(&blocked_key, &sample_partition_for_key(&blocked_key))
            .is_err());
        let failed = observe();
        assert!(failed.counter() > rejected.counter());
        assert_eq!(
            fs::read(blocked_prefix).unwrap(),
            b"foreign prefix sentinel"
        );
        // Key validation precedes even a creating producer guard.
        let absent = root.join("invalid-key-must-not-create");
        assert!(ArtifactStore::new(&absent)
            .store_partition("invalid", &partition)
            .is_err());
        assert!(!absent.exists());
        assert_eq!(observe(), failed);
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }
    fn tracked_origin_fixture(label: &str) -> (PathBuf, PathBuf) {
        use crate::storage::{
            layout::{create_fresh_reference_root, ManagedLayout},
            locks::{open_locked, LockMode},
            maintenance_ownership::{initialize_fresh_reference_domain, ReferenceDomain},
        };
        let base = unique_temp_dir(label);
        fs::create_dir_all(&base).unwrap();
        // Darwin's temp_dir may spell /var through a symlink. Positive
        // capability fixtures use the actual created root, as production gates
        // intentionally reject unresolved alias components.
        let base = fs::canonicalize(base).unwrap();
        let layout = ManagedLayout::new(base.join("storage"));
        let fresh = create_fresh_reference_root(layout.storage_root())
            .unwrap()
            .unwrap();
        layout.ensure_roots().unwrap();
        let writer = open_locked(layout.writer_lock_path(), LockMode::Exclusive).unwrap();
        let domain = ReferenceDomain::managed(&layout).unwrap();
        assert!(initialize_fresh_reference_domain(&domain, &fresh, &writer).unwrap());
        drop(writer);
        (base, layout.artifacts_root().to_path_buf())
    }
    fn observed_origin(
        root: &std::path::Path,
        key: &str,
    ) -> crate::storage::maintenance_origin::OriginObservation {
        use crate::storage::{
            locks::{path_identity, RootedDirectory},
            maintenance_budget::CleanupMemory,
            maintenance_origin::observe_artifact_origin,
        };
        let root = RootedDirectory::open_readonly_root(root, path_identity(root).unwrap()).unwrap();
        let directory = root
            .open_directory_readonly(&key.as_bytes()[..2])
            .unwrap()
            .open_directory_readonly(key.as_bytes())
            .unwrap();
        let payload = directory
            .open_readonly_file(ENVELOPE_FILE_NAME.as_bytes())
            .unwrap();
        let memory = CleanupMemory::new(128 * 1024).unwrap();
        let floor = memory.used();
        let result =
            observe_artifact_origin(&root, &directory, &payload, decode_key(key), &memory).unwrap();
        assert_eq!(memory.used(), floor);
        result
    }
    #[test]
    fn dc_origin_parallel_parser_publications_both_retain_real_creation_receipts() {
        use crate::storage::maintenance_origin::{OriginObservation, OriginPhase};
        let (base, root) = tracked_origin_fixture("dc-origin-parallel");
        let store = ArtifactStore::new(&root);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut workers = Vec::new();
        for key in ["a".repeat(64), "b".repeat(64)] {
            let store = store.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                let partition = sample_partition_for_key(&key);
                barrier.wait();
                assert_eq!(
                    store.store_partition(&key, &partition).unwrap(),
                    ArtifactWriteOutcome::Written
                );
                key
            }));
        }
        for worker in workers {
            let key = worker.join().unwrap();
            assert!(
                matches!(observed_origin(&root,&key),OriginObservation::Observed(facts) if facts.phase==OriginPhase::Published && facts.directory_created && facts.payload_created)
            );
        }
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_actual_fresh_publication_reuse_bounded_receipt_and_old_load_no_attestation() {
        use crate::storage::maintenance_origin::{OriginObservation, OriginPhase};
        let (base, root) = tracked_origin_fixture("dc-origin-real-fresh");
        let store = ArtifactStore::new(&root);
        let key = "e".repeat(64);
        let partition = sample_partition_for_key(&key);
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Written
        );
        let OriginObservation::Observed(facts) = observed_origin(&root, &key) else {
            panic!("fresh real producer must attest")
        };
        assert_eq!(facts.phase, OriginPhase::Published);
        assert!(facts.directory_created && facts.payload_created);
        assert!(!facts.torn_tail);
        let dir = root.join("ee").join(&key);
        let receipt = dir.join(".entry-origin");
        let bytes = fs::read(&receipt).unwrap();
        assert_eq!(bytes.len(), 2048);
        let identity = crate::storage::locks::path_identity(&receipt).unwrap();
        for _ in 0..3 {
            assert_eq!(
                store.store_partition(&key, &partition).unwrap(),
                ArtifactWriteOutcome::Reused
            );
            assert_eq!(fs::read(&receipt).unwrap(), bytes);
            assert_eq!(
                crate::storage::locks::path_identity(&receipt).unwrap(),
                identity
            );
        }
        fs::remove_file(&receipt).unwrap();
        fs::write(dir.join("foreign-neighbor"), b"keep").unwrap();
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_some());
        assert!(!receipt.exists());
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Reused
        );
        let OriginObservation::Observed(reused) = observed_origin(&root, &key) else {
            panic!("fresh exact equal reuse must attest leaf")
        };
        assert_eq!(reused.phase, OriginPhase::Published);
        assert!(!reused.directory_created && !reused.payload_created);
        assert_eq!(reused.payload, facts.payload);
        assert_eq!(fs::read(dir.join("foreign-neighbor")).unwrap(), b"keep");
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_missing_payload_is_republished_without_adopting_retained_directory() {
        use crate::storage::maintenance_origin::OriginObservation;
        let (base, root) = tracked_origin_fixture("dc-origin-republish-missing");
        let store = ArtifactStore::new(&root);
        let key = "b".repeat(64);
        let partition = sample_partition_for_key(&key);
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Written
        );
        let dir = root.join("bb").join(&key);
        let receipt = dir.join(".entry-origin");
        let payload = dir.join("partition.json");
        let directory_id = crate::storage::locks::path_identity(&dir).unwrap();
        let receipt_id = crate::storage::locks::path_identity(&receipt).unwrap();
        let receipt_bytes = fs::read(&receipt).unwrap();
        let OriginObservation::Observed(facts) = observed_origin(&root, &key) else {
            panic!("actual producer must establish the original receipt");
        };
        assert_eq!(
            crate::storage::locks::created_file_nonce(&File::open(&receipt).unwrap()).unwrap(),
            Some(facts.operation_nonce)
        );

        // Keep the removed inode open so the fresh producer cannot receive its
        // identity through immediate filesystem inode reuse.
        let old_payload = File::open(&payload).unwrap();
        let old_payload_id = crate::storage::locks::file_identity(&old_payload).unwrap();
        fs::remove_file(&payload).unwrap();
        let neighbor = dir.join("foreign-neighbor");
        fs::write(&neighbor, b"keep at original path").unwrap();
        let neighbor_id = crate::storage::locks::path_identity(&neighbor).unwrap();
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Written
        );
        assert_eq!(
            crate::storage::locks::path_identity(&dir).unwrap(),
            directory_id
        );
        assert_eq!(
            crate::storage::locks::path_identity(&receipt).unwrap(),
            receipt_id
        );
        assert_eq!(fs::read(&receipt).unwrap(), receipt_bytes);
        assert_eq!(
            crate::storage::locks::path_identity(&neighbor).unwrap(),
            neighbor_id
        );
        assert_eq!(fs::read(&neighbor).unwrap(), b"keep at original path");
        assert_ne!(
            crate::storage::locks::path_identity(&payload).unwrap(),
            old_payload_id
        );
        assert!(matches!(
            observed_origin(&root, &key),
            OriginObservation::Unknown(_)
        ));
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust",
                }
            )
            .unwrap()
            .is_some());
        drop(old_payload);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn dc_origin_missing_payload_refuses_unstamped_or_spoofed_receipt_birth() {
        use crate::storage::locks::{file_identity, FileIdentity};
        let (base, root) = tracked_origin_fixture("dc-origin-republish-spoof");
        let store = ArtifactStore::new(&root);
        let seed = "a".repeat(64);
        store
            .store_partition(&seed, &sample_partition_for_key(&seed))
            .unwrap();
        let seed_receipt = root.join("aa").join(&seed).join(".entry-origin");
        let original = fs::read(&seed_receipt).unwrap();
        for forged_marker in [false, true] {
            let key = if forged_marker {
                "c".repeat(64)
            } else {
                "b".repeat(64)
            };
            let dir = root.join(&key[..2]).join(&key);
            fs::create_dir_all(&dir).unwrap();
            let directory_id = crate::storage::locks::path_identity(&dir).unwrap();
            let receipt_path = dir.join(".entry-origin");
            let mut receipt = File::create(&receipt_path).unwrap();
            let receipt_id = file_identity(&receipt).unwrap();
            let mut spoofed = original.clone();
            let put_identity = |out: &mut [u8], id: FileIdentity| {
                let (volume, index) = id.fixed_words();
                out[..8].copy_from_slice(&volume.to_le_bytes());
                out[8..16].copy_from_slice(&index.to_le_bytes());
            };
            for image in spoofed.chunks_exact_mut(512) {
                image[64..96].copy_from_slice(&decode_key(&key));
                put_identity(&mut image[208..224], directory_id);
                put_identity(&mut image[240..256], receipt_id);
                let checksum = Sha256::digest(&image[..480]);
                image[480..].copy_from_slice(&checksum);
            }
            receipt.write_all(&spoofed).unwrap();
            if forged_marker {
                // A marker exists, but the actual creator nonce does not match
                // the operation copied into this otherwise rebound chain.
                crate::storage::locks::set_created_file_nonce(&receipt, &[91; 32]).unwrap();
            }
            let neighbor = dir.join("foreign-neighbor");
            fs::write(&neighbor, b"keep").unwrap();
            let neighbor_id = crate::storage::locks::path_identity(&neighbor).unwrap();
            assert!(store
                .store_partition(&key, &sample_partition_for_key(&key))
                .is_err());
            assert!(!dir.join("partition.json").exists());
            assert_eq!(
                crate::storage::locks::path_identity(&dir).unwrap(),
                directory_id
            );
            assert_eq!(
                crate::storage::locks::path_identity(&receipt_path).unwrap(),
                receipt_id
            );
            assert_eq!(fs::read(&receipt_path).unwrap(), spoofed);
            assert_eq!(
                crate::storage::locks::path_identity(&neighbor).unwrap(),
                neighbor_id
            );
            assert_eq!(fs::read(&neighbor).unwrap(), b"keep");
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn dc_origin_unproven_empty_or_different_canonical_is_never_moved_or_overwritten() {
        let (base, root) = tracked_origin_fixture("dc-origin-unproven");
        let store = ArtifactStore::new(&root);
        let seed = "a".repeat(64);
        store
            .store_partition(&seed, &sample_partition_for_key(&seed))
            .unwrap();
        let key = "b".repeat(64);
        let dir = root.join("bb").join(&key);
        fs::create_dir_all(&dir).unwrap();
        let id = crate::storage::locks::path_identity(&dir).unwrap();
        assert!(store
            .store_partition(&key, &sample_partition_for_key(&key))
            .is_err());
        assert_eq!(crate::storage::locks::path_identity(&dir).unwrap(), id);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::write(dir.join("partition.json"), b"foreign payload").unwrap();
        fs::write(dir.join("neighbor"), b"foreign sibling").unwrap();
        let payload_id = crate::storage::locks::path_identity(&dir.join("partition.json")).unwrap();
        assert!(store
            .store_partition(&key, &sample_partition_for_key(&key))
            .is_err());
        assert_eq!(
            fs::read(dir.join("partition.json")).unwrap(),
            b"foreign payload"
        );
        assert_eq!(
            crate::storage::locks::path_identity(&dir.join("partition.json")).unwrap(),
            payload_id
        );
        assert_eq!(fs::read(dir.join("neighbor")).unwrap(), b"foreign sibling");
        assert!(!dir.join(".entry-origin").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_created_temp_foreign_neighbor_stays_at_original_name_on_failure() {
        let (base, root) = tracked_origin_fixture("dc-origin-temp-neighbor");
        let store = ArtifactStore::new(&root);
        let key = "c".repeat(64);
        PRODUCER_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|stage, directory| {
                if stage == "prepared" {
                    let mut foreign = directory.create_new(b"foreign-neighbor").unwrap();
                    foreign.write_all(b"keep at original address").unwrap();
                }
            }))
        });
        let result = store.store_partition(&key, &sample_partition_for_key(&key));
        PRODUCER_HOOK.with(|hook| hook.borrow_mut().take());
        assert!(result.is_err());
        assert!(!root.join("cc").join(&key).exists());
        let temp = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".tmp-")
            })
            .unwrap();
        assert_eq!(
            fs::read(temp.join("foreign-neighbor")).unwrap(),
            b"keep at original address"
        );
        assert!(!temp.join("partition.json").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_optional_busy_publisher_still_writes_rooted_untracked_payload() {
        use crate::storage::locks::open_existing_exclusive;
        use crate::storage::maintenance_origin::OriginObservation;
        let (base, root) = tracked_origin_fixture("dc-origin-optional-publisher");
        let store = ArtifactStore::new(&root);
        let first = "d".repeat(64);
        store
            .store_partition(&first, &sample_partition_for_key(&first))
            .unwrap();
        let selector = open_existing_exclusive(&root.join(".publication.selector")).unwrap();
        let key = "f".repeat(64);
        assert_eq!(
            store
                .store_partition(&key, &sample_partition_for_key(&key))
                .unwrap(),
            ArtifactWriteOutcome::Written
        );
        drop(selector);
        assert_eq!(observed_origin(&root, &key), OriginObservation::Missing);
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_some());
        assert!(!root.join("ff").join(&key).join(".entry-origin").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_optional_receipt_completion_failure_preserves_actual_published_payload() {
        let (base, root) = tracked_origin_fixture("dc-origin-postpublish-tracking");
        let store = ArtifactStore::new(&root);
        let key = "a".repeat(64);
        PRODUCER_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|stage, directory| {
                if stage == "published" {
                    directory
                        .open_existing(b".entry-origin")
                        .unwrap()
                        .set_len(0)
                        .unwrap();
                }
            }))
        });
        let result = store.store_partition(&key, &sample_partition_for_key(&key));
        PRODUCER_HOOK.with(|hook| hook.borrow_mut().take());
        assert_eq!(result.unwrap(), ArtifactWriteOutcome::Written);
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_some());
        assert!(matches!(
            observed_origin(&root, &key),
            crate::storage::maintenance_origin::OriginObservation::Unknown(_)
        ));
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn dc_origin_completed_receipt_remains_origin_only_for_nonfinite_loader_failure() {
        let (base, root) = tracked_origin_fixture("dc-origin-nonfinite");
        let store = ArtifactStore::new(&root);
        let key = "9".repeat(64);
        let mut partition = sample_partition_for_key(&key);
        partition.edges[0].confidence = f64::NAN;
        assert_eq!(
            store.store_partition(&key, &partition).unwrap(),
            ArtifactWriteOutcome::Written
        );
        assert!(matches!(
            observed_origin(&root, &key),
            crate::storage::maintenance_origin::OriginObservation::Observed(
                crate::storage::maintenance_origin::OriginFacts {
                    phase: crate::storage::maintenance_origin::OriginPhase::Published,
                    ..
                }
            )
        ));
        assert!(store
            .load_partition(
                &key,
                &ArtifactExpectations {
                    path: "src/lib.rs",
                    content_hash: "content-hash",
                    language: "rust"
                }
            )
            .unwrap()
            .is_none());
        fs::remove_dir_all(base).unwrap();
    }
}
