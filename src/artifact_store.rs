use crate::error::NativeError;
use crate::partition_builder::GraphPartition;
use crate::protocol::{
    LanguageProfile, NativeSyntaxMaterializationRequest, OntologySchema, SourceSnapshot,
    PROFILE_COMPATIBILITY_VERSION,
};
use crate::storage::atomic::sync_dir;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const ARTIFACT_FORMAT_VERSION: u64 = 2;
const ENVELOPE_FILE_NAME: &str = "partition.json";
const ARTIFACT_KEY_BYTES: usize = 32;
const ARTIFACT_KEY_HEX_LEN: usize = ARTIFACT_KEY_BYTES * 2;

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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ArtifactEnvelope {
    format_version: u64,
    artifact_key: String,
    partition_payload_sha256: String,
    partition: GraphPartition,
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
        Self { root: root.into() }
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
        ensure_artifact_root(&self.root)?;
        partition
            .validate_raw_rows()
            .map_err(NativeError::InvalidInput)?;
        let artifact_dir = self.artifact_dir(artifact_key)?;
        if let Some(parent) = artifact_dir.parent() {
            fs::create_dir_all(parent)?;
            ensure_artifact_root(&self.root)?;
            ensure_directory_without_symlinks(parent)?;
            sync_dir(parent)?;
        }

        let temp_dir = self.temp_dir_path(artifact_key);
        fs::create_dir(&temp_dir)?;

        let mut stored_partition = partition.clone();
        stored_partition.set_artifact_key(artifact_key.to_string());
        let partition_payload_sha256 = partition_payload_sha256(&stored_partition)?;
        let existing_partition = stored_partition.clone();
        let envelope = ArtifactEnvelope {
            format_version: ARTIFACT_FORMAT_VERSION,
            artifact_key: artifact_key.to_string(),
            partition_payload_sha256: partition_payload_sha256.clone(),
            partition: stored_partition,
        };
        let payload = serde_json::to_vec_pretty(&envelope)?;
        let payload_path = temp_dir.join(ENVELOPE_FILE_NAME);
        write_and_sync(&payload_path, &payload)?;
        sync_dir(&temp_dir)?;
        sync_dir(temp_dir.parent().expect("temp dir parent should exist"))?;

        match fs::rename(&temp_dir, &artifact_dir) {
            Ok(()) => {
                sync_dir(
                    artifact_dir
                        .parent()
                        .expect("artifact directory parent should exist"),
                )?;
                Ok(ArtifactWriteOutcome::Written)
            }
            Err(_error) if artifact_dir.exists() => {
                if existing_artifact_matches(
                    &artifact_dir,
                    artifact_key,
                    &partition_payload_sha256,
                    &existing_partition,
                )? {
                    remove_tree_confined(&self.root, &temp_dir)?;
                    sync_dir(
                        artifact_dir
                            .parent()
                            .expect("artifact directory parent should exist"),
                    )?;
                    Ok(ArtifactWriteOutcome::Reused)
                } else {
                    replace_existing_artifact(&self.root, &artifact_dir, &temp_dir)?;
                    Ok(ArtifactWriteOutcome::Written)
                }
            }
            Err(error) => {
                let _ = remove_tree_confined(&self.root, &temp_dir);
                Err(NativeError::Io(error))
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
        ensure_artifact_root(&self.root)?;
        let artifact_dir = self.artifact_dir(artifact_key)?;
        if artifact_dir.exists() {
            ensure_directory_without_symlinks(&artifact_dir)?;
        }
        let payload_path = self.payload_path(artifact_key)?;
        let text = match fs::read_to_string(&payload_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(NativeError::Io(error)),
        };
        let envelope: ArtifactEnvelope = match serde_json::from_str(&text) {
            Ok(envelope) => envelope,
            Err(_) => return Ok(None),
        };
        if envelope.format_version != ARTIFACT_FORMAT_VERSION
            || envelope.artifact_key != artifact_key
        {
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

    pub(crate) fn delete_key(&self, artifact_key: &str) -> Result<bool, NativeError> {
        if validate_artifact_key(artifact_key).is_err() {
            return Ok(false);
        }
        let artifact_dir = self.artifact_dir(artifact_key)?;
        if !artifact_dir.exists() {
            return Ok(false);
        }
        ensure_artifact_root(&self.root)?;
        remove_tree_confined(&self.root, &artifact_dir)?;
        sync_dir(
            artifact_dir
                .parent()
                .expect("artifact directory parent should exist"),
        )?;
        Ok(true)
    }

    fn artifact_dir(&self, artifact_key: &str) -> Result<PathBuf, NativeError> {
        validate_artifact_key(artifact_key)?;
        Ok(self.root.join(&artifact_key[..2]).join(artifact_key))
    }

    fn payload_path(&self, artifact_key: &str) -> Result<PathBuf, NativeError> {
        Ok(self.artifact_dir(artifact_key)?.join(ENVELOPE_FILE_NAME))
    }

    fn temp_dir_path(&self, artifact_key: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.root.join(format!(".tmp-{artifact_key}-{nonce}"))
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

fn write_and_sync(path: &std::path::Path, bytes: &[u8]) -> Result<(), NativeError> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn partition_payload_sha256(partition: &GraphPartition) -> Result<String, NativeError> {
    let payload = serde_json::to_vec(partition)?;
    Ok(hex_lower(Sha256::digest(payload).as_ref()))
}

fn existing_artifact_matches(
    artifact_dir: &std::path::Path,
    artifact_key: &str,
    expected_payload_sha256: &str,
    expected_partition: &GraphPartition,
) -> Result<bool, NativeError> {
    ensure_directory_without_symlinks(artifact_dir)?;
    let payload_path = artifact_dir.join(ENVELOPE_FILE_NAME);
    let text = match fs::read_to_string(&payload_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(NativeError::Io(error)),
    };
    let envelope: ArtifactEnvelope = match serde_json::from_str(&text) {
        Ok(envelope) => envelope,
        Err(_) => return Ok(false),
    };
    if envelope.format_version != ARTIFACT_FORMAT_VERSION || envelope.artifact_key != artifact_key {
        return Ok(false);
    }
    if envelope.partition.validate_raw_rows().is_err() {
        return Ok(false);
    }
    if envelope.partition != *expected_partition {
        return Ok(false);
    }
    Ok(envelope.partition_payload_sha256 == expected_payload_sha256)
}

fn replace_existing_artifact(
    artifact_root: &std::path::Path,
    artifact_dir: &std::path::Path,
    temp_dir: &std::path::Path,
) -> Result<(), NativeError> {
    ensure_directory_without_symlinks(artifact_dir)?;
    ensure_directory_without_symlinks(temp_dir)?;
    let parent = artifact_dir.parent().ok_or_else(|| {
        NativeError::InvalidInput("artifact directory must have a parent".to_string())
    })?;
    let backup_dir = parent.join(format!(
        ".replaced-{}-{}",
        artifact_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("artifact"),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    fs::rename(artifact_dir, &backup_dir)?;
    match fs::rename(temp_dir, artifact_dir) {
        Ok(()) => {
            sync_dir(parent)?;
            remove_tree_confined(artifact_root, &backup_dir)?;
            sync_dir(parent)?;
            Ok(())
        }
        Err(error) => {
            if !artifact_dir.exists() {
                let _ = fs::rename(&backup_dir, artifact_dir);
                let _ = sync_dir(parent);
            }
            let _ = remove_tree_confined(artifact_root, temp_dir);
            Err(NativeError::Io(error))
        }
    }
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

fn ensure_directory_without_symlinks(path: &std::path::Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to use symlinked or non-directory artifact path {}",
            path.display()
        )));
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(NativeError::InvalidInput(format!(
                "refusing to use symlinked artifact path {}",
                entry.path().display()
            )));
        }
        if metadata.is_dir() {
            ensure_directory_without_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_tree_confined(
    artifact_root: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), NativeError> {
    ensure_artifact_root(artifact_root)?;
    let canonical_root = fs::canonicalize(artifact_root)?;
    let metadata = fs::symlink_metadata(target)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked artifact path {}",
            target.display()
        )));
    }
    let canonical_target = fs::canonicalize(target)?;
    if canonical_target == canonical_root || !canonical_target.starts_with(&canonical_root) {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove artifact path outside {}: {}",
            canonical_root.display(),
            canonical_target.display()
        )));
    }
    remove_path_without_symlinks(&canonical_target)
}

fn remove_path_without_symlinks(path: &std::path::Path) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(NativeError::InvalidInput(format!(
            "refusing to remove symlinked artifact path {}",
            path.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            remove_path_without_symlinks(&entry?.path())?;
        }
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
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
            semantic_enrichment: false,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: "/repo/.codebaseGraph/staging".to_string(),
            atomic_rebuild: false,
            strict: false,
            parallel: true,
            progress: false,
        }
    }

    fn sample_snapshot() -> SourceSnapshot {
        SourceSnapshot {
            path: "src/lib.rs".to_string(),
            absolute_path: "/repo/src/lib.rs".to_string(),
            content_hash: "content-hash".to_string(),
            language: Some("rust".to_string()),
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
                materialized_at: "unix:0".to_string(),
            },
            nodes,
            edges,
        }
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
    fn artifact_store_roundtrips_raw_partition_and_reuses_existing_payload() {
        let root = unique_temp_dir("codebase-graph-artifacts");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();
        let partition = sample_partition();

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
        store.store_partition(&key, &sample_partition()).unwrap();

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
        store.store_partition(&key, &sample_partition()).unwrap();

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

        store.store_partition(&key, &sample_partition()).unwrap();

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
        assert!(store.delete_key(&key).unwrap());
        assert!(store.list_keys().unwrap().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_existing_artifact_is_replaced_atomically_on_rebuild() {
        let root = unique_temp_dir("codebase-graph-artifacts-replace");
        let store = ArtifactStore::new(&root);
        let request = sample_request();
        let snapshot = sample_snapshot();
        let key =
            ArtifactStore::key_for_request(&request, &snapshot, request.profiles.first()).unwrap();

        assert_eq!(
            store.store_partition(&key, &sample_partition()).unwrap(),
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

        assert_eq!(
            store.store_partition(&key, &sample_partition()).unwrap(),
            ArtifactWriteOutcome::Written
        );
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
        assert_eq!(loaded.entry.artifact_key.as_deref(), Some(key.as_str()));

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
        assert!(error.to_string().contains("symlinked artifact path"));
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).unwrap(),
            "keep"
        );
        let _ = fs::remove_dir_all(root);
    }
}
