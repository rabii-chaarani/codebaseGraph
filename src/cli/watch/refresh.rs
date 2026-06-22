use super::output::{write_watch_event, write_watch_status};
use crate::{
    cli::build::{materialize_candidate_paths, MaterializeOptions},
    db_writer::is_transient_database_error,
    protocol::NativeSyntaxMaterializationResponse,
};
use std::{collections::BTreeSet, io::Write, thread, time::Duration};

const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(100);
const MAX_RETRY_DELAY: Duration = Duration::from_millis(1_000);

pub(in crate::cli) fn refresh_watch_batch<W: Write>(
    stdout: &mut W,
    backend: &str,
    materialize_options: &MaterializeOptions,
    event_count: usize,
    paths: &BTreeSet<String>,
) -> Result<bool, String> {
    refresh_watch_batch_with(stdout, backend, event_count, paths, |candidate_paths| {
        materialize_candidate_paths(materialize_options, candidate_paths)
            .map(|(_, response)| response)
    })
}

fn refresh_watch_batch_with<W: Write>(
    stdout: &mut W,
    backend: &str,
    event_count: usize,
    paths: &BTreeSet<String>,
    mut refresh: impl FnMut(Vec<String>) -> Result<NativeSyntaxMaterializationResponse, String>,
) -> Result<bool, String> {
    let mut delay = INITIAL_RETRY_DELAY;
    loop {
        match refresh(paths.iter().cloned().collect()) {
            Ok(response) => {
                write_watch_event(
                    stdout,
                    "refreshed",
                    Some(backend),
                    event_count,
                    paths.len(),
                    &response,
                )?;
                return Ok(true);
            }
            Err(error) => {
                let transient = is_transient_database_error(&error);
                write_watch_status(
                    stdout,
                    if transient { "retrying" } else { "error" },
                    backend,
                    Some(&watch_error_reason(&error)),
                )?;
                if !transient {
                    return Ok(false);
                }
                thread::sleep(delay);
                delay = delay.saturating_mul(2).min(MAX_RETRY_DELAY);
            }
        }
    }
}

fn watch_error_reason(error: &str) -> String {
    let reason = error.lines().next().unwrap_or("refresh_failed").trim();
    if reason.is_empty() {
        "refresh_failed".to_string()
    } else {
        reason
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("_")
            .chars()
            .take(160)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ManifestDiff;
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn skipped_response() -> NativeSyntaxMaterializationResponse {
        NativeSyntaxMaterializationResponse::skipped(
            BTreeMap::new(),
            ManifestDiff {
                added: Vec::new(),
                modified: Vec::new(),
                unchanged: Vec::new(),
                deleted: Vec::new(),
                force_rebuild: false,
            },
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        )
    }

    #[test]
    fn watch_error_reason_compacts_multiline_errors() {
        assert_eq!(
            watch_error_reason("IO exception: Could not set lock\nSee docs"),
            "IO_exception:_Could_not_set_lock"
        );
    }

    #[test]
    fn watch_refresh_retries_transient_errors_before_success() {
        let attempts = AtomicUsize::new(0);
        let mut output = Vec::new();
        let refreshed = refresh_watch_batch_with(
            &mut output,
            "poll",
            2,
            &BTreeSet::from(["src/lib.rs".to_string()]),
            |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("IO exception: Could not set lock on file".to_string())
                } else {
                    Ok(skipped_response())
                }
            },
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();

        assert!(refreshed);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        assert!(text.contains("watch event=retrying backend=poll"));
        assert!(text.contains("watch event=refreshed backend=poll"));
    }

    #[test]
    fn watch_refresh_reports_non_transient_errors_without_success() {
        let mut output = Vec::new();
        let refreshed = refresh_watch_batch_with(
            &mut output,
            "native",
            1,
            &BTreeSet::from(["src/lib.rs".to_string()]),
            |_| Err("parser exploded".to_string()),
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();

        assert!(!refreshed);
        assert!(text.contains("watch event=error backend=native reason=parser_exploded"));
    }
}
