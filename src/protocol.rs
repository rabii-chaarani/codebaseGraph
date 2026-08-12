use crate::error::NativeError;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) const GRAPH_BUILD_DIGEST_FORMAT_VERSION: u64 = 2;
pub(crate) const PROFILE_COMPATIBILITY_VERSION: u64 = 2;
pub(crate) const MATERIALIZATION_MANIFEST_SCHEMA_VERSION: u64 = 3;

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
    pub semantic_enrichment: bool,
    #[serde(default = "default_semantic_provider_mode")]
    pub semantic_provider_mode: String,
    #[serde(default)]
    pub schema_statements: Vec<String>,
    pub staging_dir: String,
    #[serde(default)]
    pub atomic_rebuild: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default = "default_parallel")]
    pub parallel: bool,
    #[serde(default)]
    pub progress: bool,
}

pub type MaterializationInput = NativeSyntaxMaterializationRequest;

fn default_semantic_provider_mode() -> String {
    "local_only".to_string()
}

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
            semantic_enrichment: self.semantic_enrichment,
            semantic_provider_mode: &self.semantic_provider_mode,
            schema_statements: &schema_statements,
        })?;
        Ok(hex_lower(Sha256::digest(payload).as_ref()))
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
    semantic_enrichment: bool,
    semantic_provider_mode: &'a str,
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
    #[serde(default, deserialize_with = "manifest_files_from_any")]
    pub files: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct ManifestEntry {
    pub path: String,
    pub content_hash: String,
    pub language: String,
    pub partition_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_key: Option<String>,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub edge_ids: Vec<String>,
    #[serde(default)]
    pub node_types: BTreeMap<String, String>,
    #[serde(default)]
    pub edge_types: BTreeMap<String, String>,
    #[serde(default)]
    pub materialized_at: String,
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
    pub pending_runs: usize,
    #[serde(default)]
    pub artifacts_reused: usize,
    #[serde(default)]
    pub artifacts_rebuilt: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_build_digest: Option<String>,
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
            pending_runs: 0,
            artifacts_reused: 0,
            artifacts_rebuilt: 0,
            graph_build_digest: None,
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
            pending_runs: 0,
            artifacts_reused: 0,
            artifacts_rebuilt: 0,
            graph_build_digest: None,
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
        assert_eq!(manifest.files["src/lib.rs"].artifact_key, None,);
    }

    #[test]
    fn optional_manifest_artifact_fields_roundtrip() {
        let mut manifest = NativeManifest {
            schema_version: 2,
            ontology: "code_ontology_v1".to_string(),
            parser_version: "native-test".to_string(),
            graph_build_digest: Some("digest".to_string()),
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
                    materialized_at: "unix:0".to_string(),
                },
            )]),
        };

        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: NativeManifest = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.graph_build_digest.as_deref(), Some("digest"));
        assert_eq!(
            decoded.files["src/lib.rs"].artifact_key.as_deref(),
            Some("artifact")
        );

        manifest.graph_build_digest = None;
        manifest.files.get_mut("src/lib.rs").unwrap().artifact_key = None;
        let encoded = serde_json::to_string(&manifest).unwrap();
        assert!(!encoded.contains("graph_build_digest"));
        assert!(!encoded.contains("artifact_key"));
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
        changed.semantic_enrichment = !request.semantic_enrichment;
        assert_ne!(
            baseline,
            changed.graph_build_compatibility_digest().unwrap()
        );

        let mut changed = request.clone();
        changed.semantic_provider_mode = "provider".to_string();
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
}
