use super::{
    filter::WatchEventFilter,
    native::{probe_native_watcher, run_native_watch, start_native_watcher},
    options::{WatchBackend, WatchLoopConfig, WatchOptions},
    output::{write_watch_event, write_watch_status},
    poll::run_poll_watch,
};
use crate::product_cli::{format::watch_help, materialize::materialize};
use std::{collections::VecDeque, io::Write, path::PathBuf};

pub(in crate::product_cli) fn run_watch<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<(), String> {
    let options = WatchOptions::parse(args)?;
    if options.help {
        writeln!(stdout, "{}", watch_help()).map_err(|error| error.to_string())?;
        return Ok(());
    }
    let backend = options.backend;
    let loop_config = WatchLoopConfig {
        poll_ms: options.poll_ms,
        debounce_ms: options.debounce_ms,
        max_iterations: options.max_iterations,
    };
    let once = options.once;
    let mut materialize_options = options.materialize;
    let source_root = materialize_options
        .source_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|error| format!("failed to resolve source root: {error}"))?;
    materialize_options.source_root = Some(source_root.clone());
    let filter = WatchEventFilter::from_options(&source_root, &materialize_options)?;
    if once {
        let (_, response) = materialize(&materialize_options)?;
        write_watch_event(stdout, "refreshed", None, 0, 0, &response)?;
        return Ok(());
    }
    match backend {
        WatchBackend::Poll => run_poll_watch(stdout, loop_config, &materialize_options, &filter),
        WatchBackend::Native => {
            let (watcher, rx) = start_native_watcher(&source_root)?;
            run_native_watch(
                stdout,
                loop_config,
                &materialize_options,
                &filter,
                watcher,
                rx,
                VecDeque::new(),
            )
        }
        WatchBackend::Auto => match start_native_watcher(&source_root) {
            Ok((watcher, rx)) => {
                let probe = probe_native_watcher(&source_root, &filter, &rx)?;
                if probe.delivered {
                    run_native_watch(
                        stdout,
                        loop_config,
                        &materialize_options,
                        &filter,
                        watcher,
                        rx,
                        probe.queued,
                    )
                } else {
                    drop(watcher);
                    write_watch_status(stdout, "fallback", "poll", probe.reason.as_deref())?;
                    run_poll_watch(stdout, loop_config, &materialize_options, &filter)
                }
            }
            Err(error) => {
                write_watch_status(stdout, "fallback", "poll", Some("watcher_start_failed"))?;
                let _ = error;
                run_poll_watch(stdout, loop_config, &materialize_options, &filter)
            }
        },
    }
}
