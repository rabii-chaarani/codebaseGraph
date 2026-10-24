use crate::error::NativeError;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) const GRAPH_BUILD_DIGEST_FORMAT_VERSION: u64 = 5;
pub(crate) const PROFILE_COMPATIBILITY_VERSION: u64 = 2;
pub(crate) const MATERIALIZATION_MANIFEST_SCHEMA_VERSION: u64 = 5;
pub(crate) const SEARCH_BACKEND_FORMAT_VERSION: u64 = 1;
const MAX_DATABASE_BUFFER_POOL_BYTES: u64 = 384 * 1024 * 1024;
const MIN_DATABASE_BUFFER_POOL_BYTES: u64 = 128 * 1024 * 1024;

const fn default_worker_memory_mib() -> u64 {
    crate::api::context::DEFAULT_WORKER_MEMORY_MIB
}

const fn default_rust_memory_mib() -> u64 {
    crate::api::context::DEFAULT_RUST_MEMORY_MIB
}

const fn default_spill_chunk_mib() -> u64 {
    crate::api::context::DEFAULT_SPILL_CHUNK_MIB
}

const fn default_max_parallelism() -> usize {
    crate::api::context::DEFAULT_MAX_PARALLELISM
}

#[derive(Debug, Clone, Deserialize)]
pub struct NativeSyntaxMaterializationRequest {
    pub source_root: String,
    pub repository_label: String,
    pub mode: String,
    pub parser_version: String,
    pub manifest_schema_version: u64,
    pub ontology: String,
    #[serde(default)]
    pub ontology_schema: OntologySchema,
    pub previous_manifest: Option<NativeManifest>,
    pub profiles: Vec<LanguageProfile>,
    pub excluded_parts: Vec<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub candidate_paths: Vec<String>,
    #[serde(default)]
    pub artifact_root: String,
    pub db_path: String,
    pub include_fts: bool,
    #[serde(default)]
    pub schema_statements: Vec<String>,
    pub staging_dir: String,
    #[serde(default)]
    pub atomic_rebuild: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default = "default_parallel")]
    pub parallel: bool,
    #[serde(default = "default_worker_memory_mib")]
    pub worker_memory_mib: u64,
    #[serde(default = "default_rust_memory_mib")]
    pub rust_memory_mib: u64,
    #[serde(default = "default_spill_chunk_mib")]
    pub spill_chunk_mib: u64,
    #[serde(default = "default_max_parallelism")]
    pub max_parallelism: usize,
    #[serde(default)]
    pub progress: bool,
}

pub type MaterializationInput = NativeSyntaxMaterializationRequest;

fn default_parallel() -> bool {
    true
}

impl NativeSyntaxMaterializationRequest {
    pub(crate) fn resolved_artifact_root(&self) -> PathBuf {
        if !self.artifact_root.trim().is_empty() {
            return PathBuf::from(&self.artifact_root);
        }

        let staging_dir = Path::new(&self.staging_dir);
        if let Some(parent) = staging_dir.parent() {
            return parent.join("artifacts");
        }

        let db_path = Path::new(&self.db_path);
        if let Some(parent) = db_path.parent() {
            return parent.join("artifacts");
        }

        staging_dir.join("artifacts")
    }

    pub(crate) fn graph_build_compatibility_digest(&self) -> Result<String, NativeError> {
        let mut ontology_relation_types = self.ontology_schema.relation_types.clone();
        ontology_relation_types.sort_by(|left, right| left.name.cmp(&right.name));
        for relation in &mut ontology_relation_types {
            relation.source_types.sort();
            relation.target_types.sort();
        }

        let mut profiles = self.profiles.clone();
        profiles.sort_by(|left, right| left.language.cmp(&right.language));
        for profile in &mut profiles {
            profile.suffixes.sort();
            profile.root_node_types.sort();
            profile
                .capture_mappings
                .sort_by(|left, right| left.capture_name.cmp(&right.capture_name));
            for mapping in &mut profile.capture_mappings {
                mapping.parser_node_types.sort();
                mapping.relation_types.sort();
            }
        }

        let mut excluded_parts = self.excluded_parts.clone();
        excluded_parts.sort();
        let mut include_patterns = self.include_patterns.clone();
        include_patterns.sort();
        let mut exclude_patterns = self.exclude_patterns.clone();
        exclude_patterns.sort();
        let mut ignore_patterns = self.ignore_patterns.clone();
        ignore_patterns.sort();
        let mut schema_statements = self.schema_statements.clone();
        schema_statements.sort();

        let payload = serde_json::to_vec(&GraphBuildDigestInput {
            format_version: GRAPH_BUILD_DIGEST_FORMAT_VERSION,
            profile_compatibility_version: PROFILE_COMPATIBILITY_VERSION,
            source_root: &self.source_root,
            repository_label: &self.repository_label,
            manifest_schema_version: self.manifest_schema_version,
            ontology: &self.ontology,
            ontology_relation_types: &ontology_relation_types,
            parser_version: &self.parser_version,
            profiles: &profiles,
            excluded_parts: &excluded_parts,
            include_patterns: &include_patterns,
            exclude_patterns: &exclude_patterns,
            ignore_patterns: &ignore_patterns,
            include_fts: self.include_fts,
            search_backend_format_version: self
                .include_fts
                .then_some(SEARCH_BACKEND_FORMAT_VERSION),
            schema_statements: &schema_statements,
        })?;
        Ok(hex_lower(Sha256::digest(payload).as_ref()))
    }

    pub(crate) fn rust_memory_limit_bytes(&self) -> Result<u64, NativeError> {
        self.rust_memory_mib
            .checked_mul(1024 * 1024)
            .ok_or_else(|| {
                NativeError::InvalidInput(
                    "rust_memory_mib is too large to convert to bytes".to_string(),
                )
            })
    }

    pub(crate) fn database_buffer_pool_bytes(&self) -> Result<u64, NativeError> {
        let worker_bytes = self
            .worker_memory_mib
            .checked_mul(1024 * 1024)
            .ok_or_else(|| {
                NativeError::InvalidInput(
                    "worker_memory_mib is too large to convert to bytes".to_string(),
                )
            })?;
        let rust_bytes = self.rust_memory_limit_bytes()?;
        let available = worker_bytes.checked_sub(rust_bytes).ok_or_else(|| {
            NativeError::InvalidInput(
                "worker_memory_mib must exceed rust_memory_mib for database writes".to_string(),
            )
        })?;
        if available < MIN_DATABASE_BUFFER_POOL_BYTES {
            return Err(NativeError::InvalidInput(
                "worker_memory_mib must reserve at least 128 MiB beyond rust_memory_mib for database writes"
                    .to_string(),
            ));
        }
        Ok(available.min(MAX_DATABASE_BUFFER_POOL_BYTES))
    }

    pub(crate) fn validate_resource_limits(&self) -> Result<(), NativeError> {
        if self.worker_memory_mib == 0
            || self.rust_memory_mib == 0
            || self.spill_chunk_mib == 0
            || self.max_parallelism == 0
        {
            return Err(NativeError::InvalidInput(
                "materialization memory limits and max_parallelism must be positive".to_string(),
            ));
        }
        if self.rust_memory_mib > self.worker_memory_mib {
            return Err(NativeError::InvalidInput(
                "rust_memory_mib must not exceed worker_memory_mib".to_string(),
            ));
        }
        self.database_buffer_pool_bytes()?;
        if self.spill_chunk_mib > self.rust_memory_mib {
            return Err(NativeError::InvalidInput(
                "spill_chunk_mib must not exceed rust_memory_mib".to_string(),
            ));
        }
        self.rust_memory_limit_bytes()?;
        Ok(())
    }
}

#[derive(Serialize)]
struct GraphBuildDigestInput<'a> {
    format_version: u64,
    profile_compatibility_version: u64,
    source_root: &'a str,
    repository_label: &'a str,
    manifest_schema_version: u64,
    ontology: &'a str,
    ontology_relation_types: &'a [OntologyRelationType],
    parser_version: &'a str,
    profiles: &'a [LanguageProfile],
    excluded_parts: &'a [String],
    include_patterns: &'a [String],
    exclude_patterns: &'a [String],
    ignore_patterns: &'a [String],
    include_fts: bool,
    search_backend_format_version: Option<u64>,
    schema_statements: &'a [String],
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OntologySchema {
    #[serde(default)]
    pub relation_types: Vec<OntologyRelationType>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OntologyRelationType {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_types: Vec<String>,
    #[serde(default)]
    pub target_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct NativeManifest {
    pub schema_version: u64,
    pub ontology: String,
    pub parser_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_build_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_backend: Option<SearchBackendMetadata>,
    #[serde(default, deserialize_with = "manifest_files_from_any")]
    pub files: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct SearchBackendMetadata {
    pub backend: String,
    pub format_version: u64,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub document_count: u64,
    #[serde(default)]
    pub term_count: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct ManifestEntry {
    pub path: String,
    pub content_hash: String,
    pub language: String,
    pub partition_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_types: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub edge_types: BTreeMap<String, String>,
    #[serde(default)]
    pub node_count: usize,
    #[serde(default)]
    pub edge_count: usize,
    #[serde(default)]
    pub materialized_at: String,
}

impl ManifestEntry {
    pub(crate) fn compact(mut self) -> Self {
        if self.node_count == 0 {
            self.node_count = self.node_ids.len();
        }
        if self.edge_count == 0 {
            self.edge_count = self.edge_ids.len();
        }
        self.node_ids.clear();
        self.edge_ids.clear();
        self.node_types.clear();
        self.edge_types.clear();
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LanguageProfile {
    pub language: String,
    #[serde(default)]
    pub suffixes: Vec<String>,
    #[serde(default)]
    pub grammar_package: String,
    pub grammar_version: String,
    pub root_node_types: Vec<String>,
    #[serde(default)]
    pub capture_mappings: Vec<CaptureMapping>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaptureMapping {
    pub capture_name: String,
    #[serde(default)]
    pub parser_node_types: Vec<String>,
    pub target_node_type: String,
    #[serde(default)]
    pub relation_types: Vec<String>,
    #[serde(default)]
    pub context_rule: String,
    #[serde(default)]
    pub construct: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceSnapshot {
    pub path: String,
    pub absolute_path: String,
    pub content_hash: String,
    pub language: Option<String>,
    #[serde(default)]
    pub byte_len: u64,
    #[serde(skip)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestDiff {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub unchanged: Vec<String>,
    pub deleted: Vec<String>,
    pub force_rebuild: bool,
}

impl ManifestDiff {
    pub fn rebuild_paths(&self) -> Vec<String> {
        let mut paths = self.added.clone();
        paths.extend(self.modified.clone());
        paths.sort();
        paths
    }
}

/// Compact cached maintenance measurements. Unknown bytes remain JSON null.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaintenanceByteBucket {
    pub logical_bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
}

/// Fixed categories make absence of an observation explicit for every category.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaintenanceCategoryBytes {
    pub active_graph: MaintenanceByteBucket,
    pub active_cache: MaintenanceByteBucket,
    pub leased_generation: MaintenanceByteBucket,
    pub live_run: MaintenanceByteBucket,
    pub recovery_evidence: MaintenanceByteBucket,
    pub artifact_temporary: MaintenanceByteBucket,
    pub legacy_staging: MaintenanceByteBucket,
    pub retired_direct: MaintenanceByteBucket,
    pub coordination: MaintenanceByteBucket,
    pub unknown: MaintenanceByteBucket,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaintenanceBytes {
    pub total: MaintenanceByteBucket,
    pub retained: MaintenanceByteBucket,
    pub potential_reclamation: MaintenanceByteBucket,
    pub deferred: MaintenanceByteBucket,
    pub unknown: MaintenanceByteBucket,
    pub by_category: MaintenanceCategoryBytes,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceInventoryStatus {
    Complete,
    Partial,
    Busy,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAttemptStatus {
    Complete,
    Partial,
    Deferred,
    Missing,
    Invalid,
}

/// Metrics are unknown until this attempt completes. A running attempt must not
/// reuse metrics from a prior attempt with its newer start time.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaintenanceAttempt {
    pub started_at_unix_ms: u64,
    pub finished_at_unix_ms: Option<u64>,
    pub status: Option<MaintenanceAttemptStatus>,
    pub attempted_entries: Option<usize>,
    pub deleted_entries: Option<usize>,
    pub reclaimed_allocated_bytes: Option<u64>,
    pub continuation_phase: Option<String>,
}

/// Additive health/materialization presentation, independent of legacy
/// `cleanup_pending`. This contains no inventory entries or storage authority.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaintenanceSummary {
    pub requested: bool,
    pub running: bool,
    pub pending: bool,
    pub observed_at_unix_ms: Option<u64>,
    pub observation_age_ms: Option<u64>,
    pub inventory_status: Option<MaintenanceInventoryStatus>,
    pub measurement_complete: bool,
    pub observation_complete: bool,
    pub bytes: MaintenanceBytes,
    pub reasons: Vec<String>,
    pub errors: Vec<String>,
    pub last_attempt: Option<MaintenanceAttempt>,
}

impl Default for MaintenanceSummary {
    fn default() -> Self {
        Self {
            requested: false,
            running: false,
            pending: true,
            observed_at_unix_ms: None,
            observation_age_ms: None,
            inventory_status: None,
            measurement_complete: false,
            observation_complete: false,
            bytes: MaintenanceBytes::default(),
            reasons: vec!["maintenance_not_observed".to_string()],
            errors: Vec::new(),
            last_attempt: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NativeSyntaxMaterializationResponse {
    pub snapshots: BTreeMap<String, SourceSnapshot>,
    pub diff: ManifestDiff,
    pub diagnostics: Vec<String>,
    pub rebuilt_entries: BTreeMap<String, ManifestEntry>,
    #[serde(default)]
    pub materialized_entries: BTreeMap<String, ManifestEntry>,
    pub copy_statements: Vec<String>,
    pub node_rows: usize,
    pub edge_rows: usize,
    pub connector_rows: usize,
    pub copy_calls: usize,
    pub graph_summary: GraphSummary,
    pub progress_events: Vec<ProgressEvent>,
    pub phase_timings: BTreeMap<String, f64>,
    pub skipped: bool,
    pub database_written: bool,
    #[serde(default)]
    pub storage_format: String,
    #[serde(default)]
    pub active_generation: Option<String>,
    #[serde(default)]
    pub cleanup_pending: bool,
    #[serde(default)]
    pub maintenance: Option<MaintenanceSummary>,
    #[serde(default)]
    pub pending_runs: usize,
    #[serde(default)]
    pub artifacts_reused: usize,
    #[serde(default)]
    pub artifacts_rebuilt: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_build_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_backend: Option<SearchBackendMetadata>,
    #[serde(default)]
    pub phase_high_water_marks: BTreeMap<String, u64>,
    #[serde(default)]
    pub spill_bytes: u64,
}

pub type MaterializationResult = NativeSyntaxMaterializationResponse;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressEvent {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub path: Option<String>,
}

impl NativeSyntaxMaterializationResponse {
    pub fn skipped(
        snapshots: BTreeMap<String, SourceSnapshot>,
        diff: ManifestDiff,
        diagnostics: Vec<String>,
        progress_events: Vec<ProgressEvent>,
        phase_timings: BTreeMap<String, f64>,
    ) -> Self {
        Self {
            snapshots,
            diff,
            diagnostics,
            rebuilt_entries: BTreeMap::new(),
            materialized_entries: BTreeMap::new(),
            copy_statements: Vec::new(),
            node_rows: 0,
            edge_rows: 0,
            connector_rows: 0,
            copy_calls: 0,
            graph_summary: GraphSummary::default(),
            progress_events,
            phase_timings,
            skipped: true,
            database_written: false,
            storage_format: String::new(),
            active_generation: None,
            cleanup_pending: false,
            maintenance: None,
            pending_runs: 0,
            artifacts_reused: 0,
            artifacts_rebuilt: 0,
            graph_build_digest: None,
            search_backend: None,
            phase_high_water_marks: BTreeMap::new(),
            spill_bytes: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        snapshots: BTreeMap<String, SourceSnapshot>,
        diff: ManifestDiff,
        diagnostics: Vec<String>,
        rebuilt_entries: BTreeMap<String, ManifestEntry>,
        materialized_entries: BTreeMap<String, ManifestEntry>,
        graph_summary: GraphSummary,
        staging: crate::staging_writer::StagingResult,
        phase_timings: BTreeMap<String, f64>,
    ) -> Self {
        let phase_high_water_marks = BTreeMap::from([(
            "staging".to_string(),
            u64::try_from(staging.high_water_bytes).unwrap_or(u64::MAX),
        )]);
        let spill_bytes = staging.spill_bytes;
        Self {
            snapshots,
            diff,
            diagnostics,
            rebuilt_entries,
            materialized_entries,
            copy_statements: staging.copy_statements,
            node_rows: staging.node_rows,
            edge_rows: staging.edge_rows,
            connector_rows: staging.connector_rows,
            copy_calls: staging.copy_calls,
            graph_summary,
            progress_events: Vec::new(),
            phase_timings,
            skipped: false,
            database_written: false,
            storage_format: String::new(),
            active_generation: None,
            cleanup_pending: false,
            maintenance: None,
            pending_runs: 0,
            artifacts_reused: 0,
            artifacts_rebuilt: 0,
            graph_build_digest: None,
            search_backend: None,
            phase_high_water_marks,
            spill_bytes,
        }
    }
}

fn manifest_files_from_any<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ManifestEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Null => Ok(BTreeMap::new()),
        Value::Array(items) => {
            let mut files = BTreeMap::new();
            for item in items {
                let entry = ManifestEntry::deserialize(item).map_err(D::Error::custom)?;
                files.insert(entry.path.clone(), entry);
            }
            Ok(files)
        }
        Value::Object(values) => values
            .into_iter()
            .map(|(path, value)| {
                let mut entry = ManifestEntry::deserialize(value).map_err(D::Error::custom)?;
                if entry.path.is_empty() {
                    entry.path = path.clone();
                }
                Ok((path, entry))
            })
            .collect(),
        _ => Err(D::Error::custom("manifest files must be a list or object")),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CaptureMapping, LanguageProfile, ManifestEntry, NativeManifest,
        NativeSyntaxMaterializationRequest, OntologyRelationType, OntologySchema,
        SearchBackendMetadata, SEARCH_BACKEND_FORMAT_VERSION,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn request_json(parallel_field: &str) -> String {
        format!(
            r#"{{
  "source_root": "/repo",
  "repository_label": "repo",
  "mode": "changed",
  "parser_version": "native-test",
  "manifest_schema_version": 1,
  "ontology": "code_ontology_v1",
  "profiles": [],
  "excluded_parts": [],
  "artifact_root": "",
  "db_path": "/repo/.codebaseGraph/graph.lbug",
  "include_fts": true,
  "staging_dir": "/repo/.codebaseGraph/native-staging"{parallel_field}
}}"#
        )
    }

    #[test]
    fn native_materialization_request_defaults_parallel_to_true() {
        let request: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json("")).unwrap();

        assert!(request.parallel);
        assert_eq!(request.worker_memory_mib, 768);
        assert_eq!(request.rust_memory_mib, 384);
        assert_eq!(request.spill_chunk_mib, 32);
        assert_eq!(request.max_parallelism, 2);
        assert_eq!(
            request.database_buffer_pool_bytes().unwrap(),
            384 * 1024 * 1024
        );
        assert_eq!(
            request.resolved_artifact_root(),
            PathBuf::from("/repo/.codebaseGraph/artifacts")
        );
    }

    #[test]
    fn native_materialization_request_preserves_explicit_parallel_false() {
        let request: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json(r#", "parallel": false"#)).unwrap();

        assert!(!request.parallel);
    }

    #[test]
    fn native_materialization_request_rejects_inconsistent_resource_limits() {
        let mut request: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json("")).unwrap();
        request.rust_memory_mib = request.worker_memory_mib + 1;

        let error = request.validate_resource_limits().unwrap_err();

        assert!(error
            .to_string()
            .contains("rust_memory_mib must not exceed worker_memory_mib"));
    }

    #[test]
    fn native_materialization_request_reserves_worker_memory_for_database_writes() {
        let mut request: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json("")).unwrap();
        request.worker_memory_mib = request.rust_memory_mib;

        let error = request.validate_resource_limits().unwrap_err();

        assert!(error.to_string().contains("must reserve at least 128 MiB"));
    }

    #[test]
    fn language_profile_requires_grammar_version_when_deserialized() {
        let profile = serde_json::from_str::<LanguageProfile>(
            r#"{
                "language": "custom",
                "grammar_package": "custom_grammar",
                "root_node_types": [],
                "capture_mappings": []
            }"#,
        );

        assert!(profile.is_err());
    }

    #[test]
    fn legacy_manifest_deserializes_without_optional_artifact_fields() {
        let manifest: NativeManifest = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "ontology": "code_ontology_v1",
                "parser_version": "native-test",
                "files": {
                    "src/lib.rs": {
                        "path": "src/lib.rs",
                        "content_hash": "hash",
                        "language": "rust",
                        "partition_id": "partition",
                        "node_ids": [],
                        "edge_ids": [],
                        "node_types": {},
                        "edge_types": {},
                        "materialized_at": "unix:0"
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(manifest.graph_build_digest, None);
        assert_eq!(manifest.search_backend, None);
        assert_eq!(manifest.files["src/lib.rs"].artifact_key, None,);
    }

    #[test]
    fn optional_manifest_artifact_fields_roundtrip() {
        let mut manifest = NativeManifest {
            schema_version: 2,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some("digest".to_string()),
            search_backend: None,
            files: BTreeMap::from([(
                "src/lib.rs".to_string(),
                ManifestEntry {
                    path: "src/lib.rs".to_string(),
                    content_hash: "hash".to_string(),
                    language: "rust".to_string(),
                    partition_id: "partition".to_string(),
                    artifact_key: Some("artifact".to_string()),
                    node_ids: Vec::new(),
                    edge_ids: Vec::new(),
                    node_types: BTreeMap::new(),
                    edge_types: BTreeMap::new(),
                    node_count: 0,
                    edge_count: 0,
                    materialized_at: "unix:0".to_string(),
                },
            )]),
        };

        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: NativeManifest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.graph_build_digest.as_deref(), Some("digest"));
        assert_eq!(decoded.search_backend, None);
        assert_eq!(
            decoded.files["src/lib.rs"].artifact_key.as_deref(),
            Some("artifact")
        );

        manifest.graph_build_digest = None;
        manifest.files.get_mut("src/lib.rs").unwrap().artifact_key = None;
        let encoded = serde_json::to_string(&manifest).unwrap();
        assert!(!encoded.contains("graph_build_digest"));
        assert!(!encoded.contains("artifact_key"));
        assert!(!encoded.contains("node_ids"));
        assert!(!encoded.contains("edge_ids"));
        assert!(!encoded.contains("node_types"));
        assert!(!encoded.contains("edge_types"));
    }

    #[test]
    fn manifest_search_backend_metadata_roundtrips_with_checksums() {
        let metadata = SearchBackendMetadata {
            backend: "sidecar".to_string(),
            format_version: SEARCH_BACKEND_FORMAT_VERSION,
            files: BTreeMap::from([("search.lexicon.bin".to_string(), "a".repeat(64))]),
            document_count: 3,
            term_count: 5,
            total_tokens: 8,
        };
        let encoded = serde_json::to_string(&metadata).unwrap();
        let decoded: SearchBackendMetadata = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, metadata);
    }

    #[test]
    fn compact_v5_manifest_entry_omits_artifact_local_row_identity() {
        let entry = ManifestEntry {
            path: "src/lib.rs".to_string(),
            content_hash: "hash".to_string(),
            language: "rust".to_string(),
            partition_id: "partition".to_string(),
            artifact_key: Some("artifact".to_string()),
            node_ids: vec!["node:1".to_string(), "node:2".to_string()],
            edge_ids: vec!["edge:1".to_string()],
            node_types: BTreeMap::from([("node:1".to_string(), "File".to_string())]),
            edge_types: BTreeMap::from([("edge:1".to_string(), "Contains".to_string())]),
            node_count: 0,
            edge_count: 0,
            materialized_at: "unix:0".to_string(),
        }
        .compact();

        let value = serde_json::to_value(entry).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 8);
        for key in [
            "path",
            "content_hash",
            "language",
            "partition_id",
            "artifact_key",
            "node_count",
            "edge_count",
            "materialized_at",
        ] {
            assert!(object.contains_key(key), "missing compact field {key}");
        }
        assert_eq!(object["node_count"], 2);
        assert_eq!(object["edge_count"], 1);
        assert!(!object.contains_key("node_ids"));
        assert!(!object.contains_key("edge_ids"));
        assert!(!object.contains_key("node_types"));
        assert!(!object.contains_key("edge_types"));
    }

    #[test]
    fn v4_manifest_remains_readable_for_one_upgrade_rebuild() {
        let manifest: NativeManifest = serde_json::from_str(
            r#"{
                "schema_version": 4,
                "ontology": "code_ontology_v1",
                "parser_version": "native-test",
                "graph_build_digest": "legacy-digest",
                "files": {
                    "src/lib.rs": {
                        "path": "src/lib.rs",
                        "content_hash": "hash",
                        "language": "rust",
                        "partition_id": "partition",
                        "artifact_key": "artifact",
                        "node_ids": ["node:1"],
                        "edge_ids": [],
                        "node_types": {"node:1": "File"},
                        "edge_types": {},
                        "materialized_at": "unix:0"
                    }
                }
            }"#,
        )
        .unwrap();

        assert_eq!(manifest.schema_version, 4);
        assert_eq!(manifest.files["src/lib.rs"].node_ids, ["node:1"]);
        assert_eq!(manifest.files["src/lib.rs"].node_count, 0);
    }

    #[test]
    fn compact_manifest_entry_retains_counts_without_row_identity_vectors() {
        let entry = ManifestEntry {
            path: "src/lib.rs".to_string(),
            content_hash: "hash".to_string(),
            language: "rust".to_string(),
            partition_id: "partition".to_string(),
            artifact_key: Some("artifact".to_string()),
            node_ids: vec!["node:1".to_string(), "node:2".to_string()],
            edge_ids: vec!["edge:1".to_string()],
            node_types: BTreeMap::from([("node:1".to_string(), "Function".to_string())]),
            edge_types: BTreeMap::from([("edge:1".to_string(), "Calls".to_string())]),
            node_count: 0,
            edge_count: 0,
            materialized_at: "unix:0".to_string(),
        }
        .compact();

        assert_eq!(entry.node_count, 2);
        assert_eq!(entry.edge_count, 1);
        assert!(entry.node_ids.is_empty());
        assert!(entry.edge_ids.is_empty());
        assert!(entry.node_types.is_empty());
        assert!(entry.edge_types.is_empty());
    }

    #[test]
    fn graph_build_digest_tracks_output_shaping_inputs_but_not_atomic_rebuild() {
        let request: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json("")).unwrap();
        let baseline = request.graph_build_compatibility_digest().unwrap();

        let mut changed = request.clone();
        changed.include_fts = !request.include_fts;
        assert_ne!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );

        let mut changed = request.clone();
        changed.schema_statements =
            vec!["CREATE NODE TABLE symbols(id STRING, PRIMARY KEY(id));".to_string()];
        assert_ne!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );

        let mut changed = request.clone();
        changed.ontology_schema = OntologySchema {
            relation_types: vec![OntologyRelationType {
                name: "Calls".to_string(),
                source_types: vec!["Function".to_string()],
                target_types: vec!["Function".to_string()],
            }],
        };
        assert_ne!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );

        let mut changed = request.clone();
        changed.profiles = vec![super::LanguageProfile {
            language: "rust".to_string(),
            suffixes: vec![".rs".to_string()],
            grammar_package: "tree_sitter_rust".to_string(),
            grammar_version: "tree_sitter_rust@0.24.2".to_string(),
            root_node_types: vec!["source_file".to_string()],
            capture_mappings: vec![CaptureMapping {
                capture_name: "definition.function".to_string(),
                parser_node_types: vec!["function_item".to_string()],
                target_node_type: "Function".to_string(),
                relation_types: Vec::new(),
                context_rule: String::new(),
                construct: String::new(),
            }],
        }];
        assert_ne!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );

        let mut changed = request.clone();
        changed.atomic_rebuild = !request.atomic_rebuild;
        assert_eq!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );
    }

    #[test]
    fn retired_semantic_request_keys_do_not_change_native_build_digest() {
        let current: NativeSyntaxMaterializationRequest =
            serde_json::from_str(&request_json("")).unwrap();
        let mut legacy_json: serde_json::Value = serde_json::from_str(&request_json("")).unwrap();
        let object = legacy_json.as_object_mut().unwrap();
        object.insert("semantic_enrichment".to_string(), serde_json::json!(true));
        object.insert(
            "semantic_provider_mode".to_string(),
            serde_json::json!("remote"),
        );
        let legacy: NativeSyntaxMaterializationRequest =
            serde_json::from_value(legacy_json).unwrap();

        assert_eq!(
            current.graph_build_compatibility_digest().unwrap(),
            legacy.graph_build_compatibility_digest().unwrap()
        );
    }
}

#[cfg(test)]
mod maintenance_summary_tests {
    use super::*;

    #[test]
    fn unobserved_maintenance_keeps_all_measurements_unknown() {
        let value = serde_json::to_value(MaintenanceSummary::default()).unwrap();
        assert_eq!(
            value["reasons"],
            serde_json::json!(["maintenance_not_observed"])
        );
        assert!(value["observed_at_unix_ms"].is_null());
        assert!(value["inventory_status"].is_null());
        assert!(value["last_attempt"].is_null());
        assert_eq!(value["measurement_complete"], false);
        assert_eq!(value["observation_complete"], false);
        let bytes = value["bytes"].as_object().unwrap();
        for (name, bucket) in bytes {
            if name == "by_category" {
                assert_eq!(bucket.as_object().unwrap().len(), 10);
                for bucket in bucket.as_object().unwrap().values() {
                    assert!(bucket["logical_bytes"].is_null());
                    assert!(bucket["allocated_bytes"].is_null());
                }
            } else {
                assert!(bucket["logical_bytes"].is_null());
                assert!(bucket["allocated_bytes"].is_null());
            }
        }
    }

    #[test]
    fn running_attempt_does_not_require_fabricated_metrics() {
        let summary = MaintenanceSummary {
            running: true,
            observed_at_unix_ms: Some(100),
            observation_age_ms: Some(200),
            last_attempt: Some(MaintenanceAttempt {
                started_at_unix_ms: 300,
                finished_at_unix_ms: None,
                status: None,
                attempted_entries: None,
                deleted_entries: None,
                reclaimed_allocated_bytes: None,
                continuation_phase: None,
            }),
            ..MaintenanceSummary::default()
        };
        let value = serde_json::to_value(&summary).unwrap();
        for field in [
            "finished_at_unix_ms",
            "status",
            "attempted_entries",
            "deleted_entries",
            "reclaimed_allocated_bytes",
        ] {
            assert!(value["last_attempt"][field].is_null(), "{field}");
        }
        assert_eq!(value["observed_at_unix_ms"], 100);
        assert_eq!(value["last_attempt"]["started_at_unix_ms"], 300);
        assert_eq!(
            serde_json::from_value::<MaintenanceSummary>(value).unwrap(),
            summary
        );
    }

    #[test]
    fn partial_measurements_and_deferred_outcomes_round_trip_without_zero_fill() {
        let summary = MaintenanceSummary {
            inventory_status: Some(MaintenanceInventoryStatus::Partial),
            bytes: MaintenanceBytes {
                by_category: MaintenanceCategoryBytes {
                    active_cache: MaintenanceByteBucket {
                        logical_bytes: Some(4096),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                deferred: MaintenanceByteBucket {
                    logical_bytes: Some(8192),
                    ..Default::default()
                },
                ..Default::default()
            },
            reasons: vec!["writer_in_use".into()],
            errors: vec!["fixture permission failure".into()],
            last_attempt: Some(MaintenanceAttempt {
                started_at_unix_ms: 100,
                finished_at_unix_ms: Some(200),
                status: Some(MaintenanceAttemptStatus::Deferred),
                attempted_entries: Some(1),
                deleted_entries: Some(0),
                reclaimed_allocated_bytes: None,
                continuation_phase: Some("validation_pending".into()),
            }),
            ..MaintenanceSummary::default()
        };
        let value = serde_json::to_value(&summary).unwrap();
        assert_eq!(value["inventory_status"], "partial");
        assert_eq!(value["last_attempt"]["status"], "deferred");
        assert!(value["bytes"]["deferred"]["allocated_bytes"].is_null());
        assert_eq!(
            serde_json::from_value::<MaintenanceSummary>(value).unwrap(),
            summary
        );
    }
}
