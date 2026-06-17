use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
    pub db_path: String,
    pub include_fts: bool,
    #[serde(default)]
    pub schema_statements: Vec<String>,
    pub staging_dir: String,
    #[serde(default)]
    pub atomic_rebuild: bool,
    #[serde(default)]
    pub strict: bool,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NativeManifest {
    pub schema_version: u64,
    pub ontology: String,
    pub parser_version: String,
    #[serde(default, deserialize_with = "manifest_files_from_any")]
    pub files: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestEntry {
    pub path: String,
    pub content_hash: String,
    pub language: String,
    pub partition_id: String,
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

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageProfile {
    pub language: String,
    #[serde(default)]
    pub suffixes: Vec<String>,
    #[serde(default)]
    pub grammar_package: String,
    #[serde(default)]
    pub root_node_types: Vec<String>,
    #[serde(default)]
    pub capture_mappings: Vec<CaptureMapping>,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct NativeSyntaxMaterializationResponse {
    pub snapshots: BTreeMap<String, SourceSnapshot>,
    pub diff: ManifestDiff,
    pub diagnostics: Vec<String>,
    pub rebuilt_entries: BTreeMap<String, ManifestEntry>,
    pub copy_statements: Vec<String>,
    pub node_rows: usize,
    pub edge_rows: usize,
    pub connector_rows: usize,
    pub copy_calls: usize,
    pub graph_summary: GraphSummary,
    pub phase_timings: BTreeMap<String, f64>,
    pub skipped: bool,
    pub database_written: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GraphSummary {
    pub node_count: usize,
    pub edge_count: usize,
}

impl NativeSyntaxMaterializationResponse {
    pub fn skipped(
        snapshots: BTreeMap<String, SourceSnapshot>,
        diff: ManifestDiff,
        diagnostics: Vec<String>,
        phase_timings: BTreeMap<String, f64>,
    ) -> Self {
        Self {
            snapshots,
            diff,
            diagnostics,
            rebuilt_entries: BTreeMap::new(),
            copy_statements: Vec::new(),
            node_rows: 0,
            edge_rows: 0,
            connector_rows: 0,
            copy_calls: 0,
            graph_summary: GraphSummary::default(),
            phase_timings,
            skipped: true,
            database_written: false,
        }
    }

    pub(crate) fn from_parts(
        snapshots: BTreeMap<String, SourceSnapshot>,
        diff: ManifestDiff,
        diagnostics: Vec<String>,
        rebuilt_entries: BTreeMap<String, ManifestEntry>,
        graph_summary: GraphSummary,
        staging: crate::staging_writer::StagingResult,
        phase_timings: BTreeMap<String, f64>,
    ) -> Self {
        Self {
            snapshots,
            diff,
            diagnostics,
            rebuilt_entries,
            copy_statements: staging.copy_statements,
            node_rows: staging.node_rows,
            edge_rows: staging.edge_rows,
            connector_rows: staging.connector_rows,
            copy_calls: staging.copy_calls,
            graph_summary,
            phase_timings,
            skipped: false,
            database_written: false,
        }
    }

    pub(crate) fn add_phase_timing(&mut self, phase: &str, seconds: f64) {
        self.phase_timings.insert(phase.to_string(), seconds);
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
