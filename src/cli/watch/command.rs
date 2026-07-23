use super::{
    options::{WatchBackend, WatchOptions},
    output::{write_watch_event, write_watch_status},
};
use crate::{
    api::refresh::{
        run_refresh_watch, watch_error_reason, RefreshBackend, RefreshLoopConfig,
        RefreshWatchConfig, RefreshWatchObserver,
    },
    cli::format::watch_help,
    protocol::NativeSyntaxMaterializationResponse,
};
use std::{io::Write, time::Duration};

pub(in crate::cli) fn run_watch<W: Write>(args: &[String], stdout: &mut W) -> Result<(), String> {
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
    run_refresh_watch(
        &options.materialize,
        config,
        &mut CliRefreshObserver { stdout },
    )
}

struct CliRefreshObserver<'a, W> {
    stdout: &'a mut W,
}

impl<W: Write> RefreshWatchObserver for CliRefreshObserver<'_, W> {
    fn on_success(
        &mut self,
        backend: Option<&str>,
        response: &NativeSyntaxMaterializationResponse,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String> {
        write_watch_event(
            self.stdout,
            "refreshed",
            backend,
            event_count,
            changed_paths,
            response,
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
            Some(&watch_error_reason(error)),
        )
    }

    fn on_fallback(&mut self, backend: &str, reason: &str) -> Result<(), String> {
        write_watch_status(self.stdout, "fallback", backend, Some(reason))
    }
}
