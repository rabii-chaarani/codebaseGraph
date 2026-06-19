use super::timing::elapsed_seconds;
use crate::error::NativeError;
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use crate::scan;
use std::collections::BTreeMap;
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
