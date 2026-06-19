use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::parser;
use crate::partition_builder;
use crate::profiles;
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

pub(super) fn build_partitions(
    request: &NativeSyntaxMaterializationRequest,
    scan: &scan::SourceScan,
    rebuild_paths: &[String],
) -> Result<Vec<PartitionBuildResult>, NativeError> {
    let profile_set = profiles::ProfileSet::new(&request.profiles);
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
                let Some(profile) = profile_set.profile_for_language(language) else {
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
        let Some(profile) = profile_set.profile_for_language(language) else {
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
