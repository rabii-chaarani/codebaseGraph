use crate::protocol::NativeSyntaxMaterializationResponse;
use std::io::Write;

pub(in crate::cli) fn write_watch_event<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: Option<&str>,
    event_count: usize,
    changed_paths: usize,
    response: &NativeSyntaxMaterializationResponse,
) -> Result<(), String> {
    let backend = backend
        .map(|backend| format!(" backend={backend}"))
        .unwrap_or_default();
    writeln!(
        stdout,
        "watch event={}{} event_count={} changed_paths={} rebuilt={} deleted={} skipped={} database_written={}",
        event,
        backend,
        event_count,
        changed_paths,
        response.diff.rebuild_paths().len(),
        response.diff.deleted.len(),
        response.skipped,
        response.database_written
    )
    .map_err(|error| error.to_string())
}

pub(in crate::cli) fn write_watch_status<W: Write>(
    stdout: &mut W,
    event: &str,
    backend: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    if let Some(reason) = reason {
        writeln!(
            stdout,
            "watch event={event} backend={backend} reason={reason}"
        )
    } else {
        writeln!(stdout, "watch event={event} backend={backend}")
    }
    .map_err(|error| error.to_string())
}
