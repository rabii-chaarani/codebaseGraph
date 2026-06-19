pub mod cli;
pub mod error;
mod graph_rows;
mod hash;
pub mod ladybug_writer;
mod normalize;
mod parser;
mod partition_builder;
mod profiles;
pub mod protocol;
mod scan;
mod semantic_enrichment;
mod staging_writer;
mod syntax_materializer;

use crate::error::NativeError;
use crate::protocol::{
    GraphSummary, NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse,
    ProgressEvent,
};
use std::collections::{BTreeMap, BTreeSet};
use std::thread;
use std::time::Instant;

pub fn plan_syntax_materialization(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let scan = scan::scan_source_state(request)?;
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
    Ok(NativeSyntaxMaterializationResponse::skipped(
        scan.snapshots,
        scan.diff,
        scan.diagnostics,
        Vec::new(),
        phase_timings,
    ))
}

pub fn materialize_syntax_batch(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let scan = scan::scan_source_state(request)?;
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
    let diff = scan.diff.clone();
    if diff.rebuild_paths().is_empty() && diff.deleted.is_empty() {
        return Ok(NativeSyntaxMaterializationResponse::skipped(
            scan.snapshots,
            diff,
            scan.diagnostics,
            Vec::new(),
            phase_timings,
        ));
    }

    let profile_set = profiles::ProfileSet::new(&request.profiles);
    let mut staging_accumulator = staging_writer::StagingAccumulator::new(&request.staging_dir);
    let mut rebuilt_entries = BTreeMap::new();
    let mut node_ids = BTreeSet::new();
    let mut edge_ids = BTreeSet::new();
    let mut diagnostics = scan.diagnostics;
    let mut partitions = Vec::new();
    let mut parse_seconds = 0.0;
    let mut graph_build_seconds = 0.0;
    let mut staging_seconds = 0.0;
    let mut progress_events = Vec::new();
    let rebuild_paths = diff.rebuild_paths();
    let rebuild_total = rebuild_paths.len();
    if request.parallel && rebuild_total > 1 {
        let results = thread::scope(|scope| {
            let mut handles = Vec::new();
            for path in &rebuild_paths {
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
        })?;
        for (index, result) in results.into_iter().enumerate() {
            parse_seconds += result.parse_seconds;
            graph_build_seconds += result.graph_build_seconds;
            diagnostics.extend(result.diagnostics);
            if request.progress {
                progress_events.push(ProgressEvent {
                    phase: "parsed".to_string(),
                    current: index + 1,
                    total: rebuild_total,
                    path: Some(result.partition.entry.path.clone()),
                });
            }
            partitions.push(result.partition);
        }
    } else {
        for (index, path) in rebuild_paths.iter().enumerate() {
            let Some(snapshot) = scan.supported.get(path) else {
                continue;
            };
            let Some(language) = snapshot.language.as_deref() else {
                continue;
            };
            let Some(profile) = profile_set.profile_for_language(language) else {
                continue;
            };
            let result = build_partition_for_snapshot(request, snapshot, profile)?;
            parse_seconds += result.parse_seconds;
            graph_build_seconds += result.graph_build_seconds;
            diagnostics.extend(result.diagnostics);
            if request.progress {
                progress_events.push(ProgressEvent {
                    phase: "parsed".to_string(),
                    current: index + 1,
                    total: rebuild_total,
                    path: Some(result.partition.entry.path.clone()),
                });
            }
            partitions.push(result.partition);
        }
    }
    partitions.sort_by(|left, right| left.entry.path.cmp(&right.entry.path));
    phase_timings.insert("parse_seconds".to_string(), parse_seconds);
    phase_timings.insert("graph_build_seconds".to_string(), graph_build_seconds);

    let semantic_stats = semantic_enrichment::enrich_partitions(&mut partitions, request)?;
    for (phase, seconds) in semantic_stats.phase_timings {
        phase_timings.insert(phase, seconds);
    }

    for partition in partitions {
        for node_id in &partition.entry.node_ids {
            node_ids.insert(node_id.clone());
        }
        for edge_id in &partition.entry.edge_ids {
            edge_ids.insert(edge_id.clone());
        }
        let staging_started = Instant::now();
        staging_accumulator.add_partition(&partition);
        staging_seconds += elapsed_seconds(staging_started);
        if request.progress {
            progress_events.push(ProgressEvent {
                phase: "staged".to_string(),
                current: rebuilt_entries.len() + 1,
                total: rebuild_total,
                path: Some(partition.entry.path.clone()),
            });
        }
        let entry_path = partition.entry.path.clone();
        rebuilt_entries.insert(entry_path, partition.entry);
    }

    let staging_started = Instant::now();
    let staging = staging_accumulator.finish()?;
    staging_seconds += elapsed_seconds(staging_started);
    phase_timings.insert("staging_seconds".to_string(), staging_seconds);
    let graph_summary = GraphSummary {
        node_count: node_ids.len(),
        edge_count: edge_ids.len(),
    };
    let mut response = NativeSyntaxMaterializationResponse::from_parts(
        scan.snapshots,
        diff,
        diagnostics,
        rebuilt_entries,
        graph_summary,
        staging,
        phase_timings,
    );
    response.progress_events = progress_events;
    Ok(response)
}

pub fn materialize_syntax_batch_json(payload: &str) -> Result<String, NativeError> {
    let decode_started = Instant::now();
    let request: NativeSyntaxMaterializationRequest = serde_json::from_str(payload)?;
    let json_decode_seconds = elapsed_seconds(decode_started);
    let mut response = materialize_syntax_batch(&request)?;
    response.add_phase_timing("rust_json_decode_seconds", json_decode_seconds);
    Ok(serde_json::to_string(&response)?)
}

fn elapsed_seconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64()
}

struct PartitionBuildResult {
    partition: partition_builder::GraphPartition,
    diagnostics: Vec<String>,
    parse_seconds: f64,
    graph_build_seconds: f64,
}

fn build_partition_for_snapshot(
    request: &NativeSyntaxMaterializationRequest,
    snapshot: &protocol::SourceSnapshot,
    profile: &protocol::LanguageProfile,
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
