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

pub fn materialize_syntax_batch(
    request: NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let scan = scan::scan_source_state(&request)?;
    let diff = scan.diff.clone();
    if diff.rebuild_paths().is_empty() && diff.deleted.is_empty() {
        return Ok(NativeSyntaxMaterializationResponse::skipped(
            scan.snapshots,
            diff,
            scan.diagnostics,
        ));
    }

    let profile_set = profiles::ProfileSet::new(&request.profiles);
    let mut partitions = Vec::new();
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
        let captures = parser::parse_file(snapshot, profile)?;
        partitions.push(graph::build_partition(&request, snapshot, captures)?);
    }

    let staging = staging::write_partitions(&request, &partitions)?;
    Ok(NativeSyntaxMaterializationResponse::from_parts(
        scan.snapshots,
        diff,
        scan.diagnostics,
        partitions,
        staging,
    ))
}

pub fn materialize_syntax_batch_json(payload: &str) -> Result<String, NativeError> {
    let request: NativeSyntaxMaterializationRequest = serde_json::from_str(payload)?;
    let response = materialize_syntax_batch(request)?;
    Ok(serde_json::to_string(&response)?)
}
