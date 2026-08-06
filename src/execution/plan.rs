use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use crate::scan;
use std::collections::BTreeMap;
use std::time::Instant;

pub fn plan_materialization(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    let mut phase_timings = BTreeMap::new();
    let scan_started = Instant::now();
    let mut scan = scan::scan_sources(request)?;
    if !request.candidate_paths.is_empty()
        && request
            .previous_manifest
            .as_ref()
            .is_some_and(|manifest| manifest.schema_version >= 2)
    {
        let candidates = request
            .candidate_paths
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        scan.diff
            .added
            .retain(|path| candidates.contains(path.as_str()));
        scan.diff
            .modified
            .retain(|path| candidates.contains(path.as_str()));
        scan.diff
            .deleted
            .retain(|path| candidates.contains(path.as_str()));
    }
    phase_timings.insert("scan_seconds".to_string(), elapsed_seconds(scan_started));
    Ok(NativeSyntaxMaterializationResponse::skipped(
        scan.snapshots,
        scan.diff,
        scan.diagnostics,
        Vec::new(),
        phase_timings,
    ))
}
