pub mod error;
#[cfg(feature = "python-extension")]
mod ffi;
mod graph;
mod hash;
pub mod ladybug;
pub mod legacy;
mod normalize;
mod parser;
mod profiles;
pub mod protocol;
mod scan;
mod staging;

use crate::error::NativeError;
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use std::collections::BTreeMap;
use std::time::Instant;

pub fn materialize_syntax_batch(
    request: NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let scan = scan::scan_source_state(&request)?;
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
    let diff = scan.diff.clone();
    if diff.rebuild_paths().is_empty() && diff.deleted.is_empty() {
        return Ok(NativeSyntaxMaterializationResponse::skipped(
            scan.snapshots,
            diff,
            scan.diagnostics,
            phase_timings,
        ));
    }

    let profile_set = profiles::ProfileSet::new(&request.profiles);
    let mut partitions = Vec::new();
    let mut diagnostics = scan.diagnostics;
    let mut parse_seconds = 0.0;
    let mut graph_build_seconds = 0.0;
    for path in diff.rebuild_paths() {
        let Some(snapshot) = scan.supported.get(&path) else {
            continue;
        };
        let Some(language) = snapshot.language.as_deref() else {
            continue;
        };
        let Some(profile) = profile_set.profile_for_language(language) else {
            continue;
        };
        let parse_started = Instant::now();
        let parse = parser::parse_file(snapshot, profile)?;
        parse_seconds += elapsed_seconds(parse_started);
        let mut parse_diagnostics = parse.diagnostics.clone();
        let graph_build_started = Instant::now();
        partitions.push(graph::build_partition(&request, snapshot, parse)?);
        graph_build_seconds += elapsed_seconds(graph_build_started);
        diagnostics.append(&mut parse_diagnostics);
    }
    phase_timings.insert("parse_seconds".to_string(), parse_seconds);
    phase_timings.insert("graph_build_seconds".to_string(), graph_build_seconds);

    let staging_started = Instant::now();
    let staging = staging::write_partitions(&request, &partitions)?;
    phase_timings.insert(
        "staging_seconds".to_string(),
        elapsed_seconds(staging_started),
    );
    Ok(NativeSyntaxMaterializationResponse::from_parts(
        scan.snapshots,
        diff,
        diagnostics,
        partitions,
        staging,
        phase_timings,
    ))
}

pub fn materialize_syntax_batch_json(payload: &str) -> Result<String, NativeError> {
    let decode_started = Instant::now();
    let request: NativeSyntaxMaterializationRequest = serde_json::from_str(payload)?;
    let json_decode_seconds = elapsed_seconds(decode_started);
    let mut response = materialize_syntax_batch(request)?;
    response.add_phase_timing("rust_json_decode_seconds", json_decode_seconds);
    let encode_started = Instant::now();
    let _ = serde_json::to_string(&response)?;
    response.add_phase_timing("rust_json_encode_seconds", elapsed_seconds(encode_started));
    Ok(serde_json::to_string(&response)?)
}

fn elapsed_seconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64()
}
