use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::parser;
use crate::partition_builder;
use crate::protocol::{LanguageProfile, NativeSyntaxMaterializationRequest, SourceSnapshot};
use crate::scan;
use std::thread;
use std::time::Instant;

pub(super) struct PartitionBuildResult {
    pub(super) partition: partition_builder::GraphPartition,
    pub(super) diagnostics: Vec<String>,
    pub(super) parse_seconds: f64,
    pub(super) graph_build_seconds: f64,
}

pub(super) fn build_execution_plan(
    scan: &scan::SourceScan,
    rebuild_paths: &[String],
) -> Result<Vec<PartitionBuildResult>, NativeError> {
    let request = &scan.input;
    if request.parallel && rebuild_paths.len() > 1 {
        return thread::scope(|scope| {
            let mut handles = Vec::new();
            for path in rebuild_paths {
                let Some(snapshot) = scan.supported.get(path) else {
                    continue;
                };
                let Some(language) = snapshot.language.as_deref() else {
                    continue;
                };
                let Some(profile) = scan
                    .profiles
                    .iter()
                    .find(|profile| profile.language == language)
                else {
                    continue;
                };
                handles.push(
                    scope.spawn(move || build_partition_for_snapshot(request, snapshot, profile)),
                );
            }
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().map_err(|_| {
                        NativeError::InvalidInput("parallel parser panicked".to_string())
                    })?
                })
                .collect::<Result<Vec<_>, NativeError>>()
        });
    }

    let mut results = Vec::new();
    for path in rebuild_paths {
        let Some(snapshot) = scan.supported.get(path) else {
            continue;
        };
        let Some(language) = snapshot.language.as_deref() else {
            continue;
        };
        let Some(profile) = scan
            .profiles
            .iter()
            .find(|profile| profile.language == language)
        else {
            continue;
        };
        results.push(build_partition_for_snapshot(request, snapshot, profile)?);
    }
    Ok(results)
}

fn build_partition_for_snapshot(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &SourceSnapshot,
    profile: &LanguageProfile,
) -> Result<PartitionBuildResult, NativeError> {
    let parse_started = Instant::now();
    let parse = parser::parse_file(snapshot, profile)?;
    let parse_seconds = elapsed_seconds(parse_started);
    let diagnostics = parse.diagnostics.clone();
    let graph_build_started = Instant::now();
    let partition = partition_builder::build_partition(request, snapshot, parse)?;
    let graph_build_seconds = elapsed_seconds(graph_build_started);
    Ok(PartitionBuildResult {
        partition,
        diagnostics,
        parse_seconds,
        graph_build_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{NativeSyntaxMaterializationRequest, OntologySchema};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn execution_plan_uses_scanned_source_after_repository_is_removed() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let source_root =
            std::env::temp_dir().join(format!("codebase-graph-scanned-source-{nonce}"));
        fs::create_dir_all(source_root.join("src")).expect("source directory should be created");
        fs::write(
            source_root.join("src/lib.rs"),
            "pub fn scanned_source() -> bool { true }\n",
        )
        .expect("source file should be written");

        let request = NativeSyntaxMaterializationRequest {
            source_root: source_root.to_string_lossy().into_owned(),
            repository_label: "scanned-source".to_string(),
            mode: "full".to_string(),
            parser_version: "test".to_string(),
            manifest_schema_version: 1,
            ontology: "code_ontology_v1".to_string(),
            ontology_schema: OntologySchema::default(),
            previous_manifest: None,
            profiles: Vec::new(),
            excluded_parts: Vec::new(),
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            ignore_patterns: Vec::new(),
            candidate_paths: Vec::new(),
            db_path: source_root.join("graph").to_string_lossy().into_owned(),
            include_fts: false,
            semantic_enrichment: false,
            semantic_provider_mode: "local_only".to_string(),
            schema_statements: Vec::new(),
            staging_dir: source_root.join("staging").to_string_lossy().into_owned(),
            atomic_rebuild: false,
            strict: true,
            parallel: false,
            progress: false,
        };
        let scan = crate::scan::scan_sources(&request).expect("scan should succeed");
        let rebuild_paths = scan.diff.rebuild_paths();

        fs::remove_dir_all(&source_root).expect("source repository should be removed");

        let plan = build_execution_plan(&scan, &rebuild_paths)
            .expect("planning should use the scanned source payload");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].partition.entry.path, "src/lib.rs");
    }
}
