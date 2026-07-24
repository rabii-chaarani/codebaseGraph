use super::{
    options::{WatchBackend, WatchOptions},
    output::{write_watch_event, write_watch_status},
};
use crate::adapters::cli::{format::watch_help, materialization_input::materialize_request};
use crate::api::{
    CodebaseGraphApi, OutputFormat, RefreshBackend, RefreshLoopConfig, RefreshWatchConfig,
    RefreshWatchObserver, RefreshWatchSummary,
};
use std::{io::Write, time::Duration};

pub(in crate::adapters::cli) fn run_watch<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = WatchOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", watch_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let backend = match options.backend {
        WatchBackend::Auto => RefreshBackend::Auto,
        WatchBackend::Native => RefreshBackend::Native,
        WatchBackend::Poll => RefreshBackend::Poll,
    };
    let config = RefreshWatchConfig {
        backend,
        loop_config: RefreshLoopConfig {
            poll_interval: Duration::from_millis(options.poll_ms),
            debounce: Duration::from_millis(options.debounce_ms),
            max_wait: Duration::from_secs(5).max(Duration::from_millis(
                options.debounce_ms.saturating_mul(10),
            )),
            max_iterations: options.max_iterations,
        },
        once: options.once,
    };
    let request = materialize_request(&options.materialize, OutputFormat::Typed);
    CodebaseGraphApi::new()
        .watch_repository(&request, config, &mut CliRefreshObserver { stdout })
        .map_err(|error| error.message)
}

struct CliRefreshObserver<'a, W> {
    stdout: &'a mut W,
}

impl<W: Write> RefreshWatchObserver for CliRefreshObserver<'_, W> {
    fn on_success(
        &mut self,
        backend: Option<&str>,
        summary: &RefreshWatchSummary,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        write_watch_event(
            self.stdout,
            "refreshed",
            backend,
            event_count,
            changed_paths,
            summary,
        )
    }

    fn on_error(
        &mut self,
        backend: &str,
        error: &str,
        retrying: bool,
        _event_count: usize,
        _changed_paths: usize,
    ) -> Result<(), String> {
        write_watch_status(
            self.stdout,
            if retrying { "retrying" } else { "error" },
            backend,
            Some(&error_reason(error)),
        )
    }

    fn on_fallback(&mut self, backend: &str, reason: &str) -> Result<(), String> {
        write_watch_status(self.stdout, "fallback", backend, Some(reason))
    }
}

fn error_reason(error: &str) -> String {
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
    use super::error_reason;

    #[test]
    fn watch_error_reason_compacts_multiline_errors() {
        assert_eq!(
            error_reason("IO exception: Could not set lock\nSee docs"),
            "IO_exception:_Could_not_set_lock"
        );
    }
}
