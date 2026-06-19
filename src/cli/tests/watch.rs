use super::*;

#[test]
fn watch_filter_ignores_excluded_parts_and_access_events() {
    let root = unique_temp_dir("codebase-graph-rust-watch-filter-excluded");
    fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    let filter = watch_filter_for(&root, &[]);

    let read_access = watch_test_event(
        &root,
        EventKind::Access(notify::event::AccessKind::Open(
            notify::event::AccessMode::Read,
        )),
        &["src/lib.rs"],
    );
    assert!(filter.relevant_paths(&read_access).is_empty());

    let write_close = watch_test_event(
        &root,
        EventKind::Access(notify::event::AccessKind::Close(
            notify::event::AccessMode::Write,
        )),
        &["src/lib.rs"],
    );
    assert_eq!(
        filter.relevant_paths(&write_close),
        BTreeSet::from(["src/lib.rs".to_string()])
    );

    let backend_other = watch_test_event(&root, EventKind::Other, &["src/lib.rs"]);
    assert_eq!(
        filter.relevant_paths(&backend_other),
        BTreeSet::from(["src/lib.rs".to_string()])
    );

    let state_dir = watch_test_event(
        &root,
        EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        &[".codebaseGraph/manifest.json"],
    );
    assert!(filter.relevant_paths(&state_dir).is_empty());

    let target_dir = watch_test_event(
        &root,
        EventKind::Create(notify::event::CreateKind::File),
        &["target/debug/build.log"],
    );
    assert!(filter.relevant_paths(&target_dir).is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_filter_honors_ignore_config_and_cli_excludes() {
    let root = unique_temp_dir("codebase-graph-rust-watch-filter-rules");
    fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
    fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
    fs::write(
        root.join(".codebaseGraph").join("config.json"),
        r#"{"materialization":{"exclude":["config_skip.py"]}}"#,
    )
    .unwrap();
    let filter = watch_filter_for(&root, &["--exclude", "cli_skip.py"]);

    for path in ["ignored.py", "config_skip.py", "cli_skip.py"] {
        let event = watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &[path],
        );
        assert!(filter.relevant_paths(&event).is_empty());
    }

    let event = watch_test_event(
        &root,
        EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        &["keep.py"],
    );
    assert_eq!(
        filter.relevant_paths(&event),
        BTreeSet::from(["keep.py".to_string()])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_filter_keeps_unsupported_files_when_unignored() {
    let root = unique_temp_dir("codebase-graph-rust-watch-filter-unsupported");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let event = watch_test_event(
        &root,
        EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        &["notes.txt"],
    );

    assert_eq!(
        filter.relevant_paths(&event),
        BTreeSet::from(["notes.txt".to_string()])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_filter_accepts_relative_notify_paths() {
    let root = unique_workspace_dir("codebase-graph-rust-watch-relative");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let cwd_relative_path = root
        .strip_prefix(env::current_dir().unwrap())
        .unwrap()
        .join("cwd_relative.py");

    let cwd_relative = Event {
        kind: EventKind::Create(notify::event::CreateKind::File),
        paths: vec![cwd_relative_path],
        attrs: Default::default(),
    };
    assert_eq!(
        filter.relevant_paths(&cwd_relative),
        BTreeSet::from(["cwd_relative.py".to_string()])
    );

    let root_relative = Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![PathBuf::from("root_relative.py")],
        attrs: Default::default(),
    };
    assert_eq!(
        filter.relevant_paths(&root_relative),
        BTreeSet::from(["root_relative.py".to_string()])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_batch_coalesces_burst_events_until_quiet() {
    let root = unique_temp_dir("codebase-graph-rust-watch-burst");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (tx, rx) = mpsc::channel();
    tx.send(WatchMessage::Event(watch_test_event(
        &root,
        EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        &["b.py"],
    )))
    .unwrap();
    let mut queued = VecDeque::new();

    let batch = collect_watch_batch(
        WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &["a.py"],
        )),
        &rx,
        &mut queued,
        &filter,
        Duration::from_millis(10),
        Duration::from_secs(1),
    )
    .unwrap()
    .unwrap();

    assert_eq!(batch.event_count, 2);
    assert_eq!(
        batch.paths,
        BTreeSet::from(["a.py".to_string(), "b.py".to_string()])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_batch_flushes_under_sustained_churn() {
    let root = unique_temp_dir("codebase-graph-rust-watch-churn");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (tx, rx) = mpsc::channel();
    let sender_root = root.clone();
    let sender = std::thread::spawn(move || {
        for index in 0..20 {
            tx.send(WatchMessage::Event(watch_test_event(
                &sender_root,
                EventKind::Modify(notify::event::ModifyKind::Data(
                    notify::event::DataChange::Content,
                )),
                &[&format!("churn-{index}.py")],
            )))
            .unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    let started = Instant::now();
    let mut queued = VecDeque::new();
    let batch = collect_watch_batch(
        WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Create(notify::event::CreateKind::File),
            &["initial.py"],
        )),
        &rx,
        &mut queued,
        &filter,
        Duration::from_millis(100),
        Duration::from_millis(30),
    )
    .unwrap()
    .unwrap();
    sender.join().unwrap();

    assert!(started.elapsed() < Duration::from_millis(200));
    assert!(batch.event_count > 1);
    assert!(batch.paths.contains("initial.py"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_batch_coalesces_queued_events_into_follow_up_refresh() {
    let root = unique_temp_dir("codebase-graph-rust-watch-queued");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (tx, rx) = mpsc::channel();
    for path in ["during-a.py", "during-b.py", "during-c.py"] {
        tx.send(WatchMessage::Event(watch_test_event(
            &root,
            EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            &[path],
        )))
        .unwrap();
    }
    let mut queued = VecDeque::new();

    let batch = collect_watch_batch(
        rx.recv().unwrap(),
        &rx,
        &mut queued,
        &filter,
        Duration::from_millis(10),
        Duration::from_secs(1),
    )
    .unwrap()
    .unwrap();

    assert_eq!(batch.event_count, 3);
    assert_eq!(
        batch.paths,
        BTreeSet::from([
            "during-a.py".to_string(),
            "during-b.py".to_string(),
            "during-c.py".to_string()
        ])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_batch_propagates_watcher_errors() {
    let root = unique_temp_dir("codebase-graph-rust-watch-error");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (_tx, rx) = mpsc::channel();
    let mut queued = VecDeque::new();
    let error = collect_watch_batch(
        WatchMessage::Error("backend failed".to_string()),
        &rx,
        &mut queued,
        &filter,
        Duration::from_millis(1),
        Duration::from_millis(1),
    )
    .unwrap_err();

    assert!(error.contains("filesystem watcher error: backend failed"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_probe_succeeds_when_notify_event_arrives() {
    let _guard = watch_test_env_lock();
    set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "5");
    let root = unique_temp_dir("codebase-graph-rust-watch-probe-success");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (tx, rx) = mpsc::channel();
    tx.send(WatchMessage::Event(watch_test_event(
        &root,
        EventKind::Create(notify::event::CreateKind::File),
        &[".codebaseGraph/watch-probe/probe-test.tmp"],
    )))
    .unwrap();

    let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

    assert!(outcome.delivered);
    assert!(outcome.queued.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_probe_falls_back_after_timeout() {
    let _guard = watch_test_env_lock();
    set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "1");
    let root = unique_temp_dir("codebase-graph-rust-watch-probe-timeout");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (_tx, rx) = mpsc::channel();

    let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

    assert!(!outcome.delivered);
    assert_eq!(outcome.reason.as_deref(), Some("probe_timeout"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_probe_discards_probe_events_and_queues_real_events() {
    let _guard = watch_test_env_lock();
    set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "5");
    let root = unique_temp_dir("codebase-graph-rust-watch-probe-queue");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let (tx, rx) = mpsc::channel();
    tx.send(WatchMessage::Event(watch_test_event(
        &root,
        EventKind::Create(notify::event::CreateKind::File),
        &[".codebaseGraph/watch-probe/probe-test.tmp"],
    )))
    .unwrap();
    tx.send(WatchMessage::Event(watch_test_event(
        &root,
        EventKind::Create(notify::event::CreateKind::File),
        &["src/lib.rs"],
    )))
    .unwrap();

    let outcome = probe_native_watcher(&root.canonicalize().unwrap(), &filter, &rx).unwrap();

    assert!(outcome.delivered);
    assert_eq!(outcome.queued.len(), 1);
    let mut batch = WatchChangeBatch::default();
    apply_watch_message(
        outcome.queued.into_iter().next().unwrap(),
        &filter,
        &mut batch,
    )
    .unwrap();
    assert_eq!(batch.paths, BTreeSet::from(["src/lib.rs".to_string()]));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_poll_snapshot_honors_filters() {
    let root = unique_temp_dir("codebase-graph-rust-watch-poll-filter");
    fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("keep.py"), "def keep():\n    return 1\n").unwrap();
    fs::write(root.join("ignored.py"), "def ignored():\n    return 1\n").unwrap();
    fs::write(root.join("config_skip.py"), "def skip():\n    return 1\n").unwrap();
    fs::write(root.join("cli_skip.py"), "def skip():\n    return 1\n").unwrap();
    fs::write(
        root.join("target").join("build.py"),
        "def build():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        root.join(".codebaseGraph").join("internal.py"),
        "def internal():\n    return 1\n",
    )
    .unwrap();
    fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
    fs::write(
        root.join(".codebaseGraph").join("config.json"),
        r#"{"materialization":{"exclude":["config_skip.py"]}}"#,
    )
    .unwrap();
    let filter = watch_filter_for(&root, &["--exclude", "cli_skip.py"]);

    let snapshot = watch_file_snapshot(&filter).unwrap();

    assert!(snapshot.contains_key("keep.py"));
    assert!(!snapshot.contains_key("ignored.py"));
    assert!(!snapshot.contains_key("config_skip.py"));
    assert!(!snapshot.contains_key("cli_skip.py"));
    assert!(!snapshot.contains_key("target/build.py"));
    assert!(!snapshot.contains_key(".codebaseGraph/internal.py"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_poll_snapshot_detects_create_modify_and_delete() {
    let root = unique_temp_dir("codebase-graph-rust-watch-poll-diff");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("modify.py"), "def value():\n    return 1\n").unwrap();
    fs::write(root.join("delete.py"), "def gone():\n    return 1\n").unwrap();
    let filter = watch_filter_for(&root, &[]);
    let previous = watch_file_snapshot(&filter).unwrap();

    fs::write(root.join("modify.py"), "def value():\n    return 100\n").unwrap();
    fs::write(root.join("create.py"), "def new():\n    return 2\n").unwrap();
    fs::remove_file(root.join("delete.py")).unwrap();
    let current = watch_file_snapshot(&filter).unwrap();
    let diff = watch_snapshot_diff(&previous, &current);

    assert_eq!(
        diff,
        BTreeSet::from([
            "create.py".to_string(),
            "delete.py".to_string(),
            "modify.py".to_string()
        ])
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_poll_batch_flushes_under_sustained_churn() {
    let root = unique_temp_dir("codebase-graph-rust-watch-poll-churn");
    fs::create_dir_all(&root).unwrap();
    let filter = watch_filter_for(&root, &[]);
    let mut previous = watch_file_snapshot(&filter).unwrap();
    let writer_root = root.clone();
    let writer = std::thread::spawn(move || {
        for index in 0..20 {
            fs::write(
                writer_root.join(format!("churn-{index}.py")),
                format!("def churn_{index}():\n    return {index}\n"),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    let started = Instant::now();
    let batch = collect_poll_batch(
        &filter,
        &mut previous,
        Duration::from_millis(5),
        Duration::from_millis(100),
        Duration::from_millis(30),
    )
    .unwrap();
    writer.join().unwrap();

    assert!(started.elapsed() < Duration::from_millis(200));
    assert!(batch.event_count > 1);
    assert!(!batch.paths.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_poll_backend_refreshes_after_create() {
    let root = unique_temp_dir("codebase-graph-rust-watch-poll-cli");
    fs::create_dir_all(&root).unwrap();
    let watch_root = root.clone();
    let handle = std::thread::spawn(move || {
        let mut output = Vec::new();
        run(
            [
                "watch",
                "--source-root",
                watch_root.to_str().unwrap(),
                "--watch-backend",
                "poll",
                "--poll-ms",
                "10",
                "--debounce-ms",
                "10",
                "--max-iterations",
                "1",
                "--no-git",
                "--no-fts",
                "--no-semantic-enrichment",
            ],
            &mut output,
        )
        .unwrap();
        String::from_utf8(output).unwrap()
    });
    std::thread::sleep(Duration::from_millis(30));
    fs::write(root.join("created.py"), "def created():\n    return 1\n").unwrap();
    let text = handle.join().unwrap();

    assert!(text.contains("watch event=refreshed backend=poll"));
    assert!(text.contains("changed_paths=1"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_auto_backend_falls_back_to_poll_when_probe_times_out() {
    let _guard = watch_test_env_lock();
    set_test_env("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS", "1");
    set_test_env("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE", "1");
    let root = unique_temp_dir("codebase-graph-rust-watch-auto-fallback");
    fs::create_dir_all(&root).unwrap();
    let watch_root = root.clone();
    let handle = std::thread::spawn(move || {
        let mut output = Vec::new();
        run(
            [
                "watch",
                "--source-root",
                watch_root.to_str().unwrap(),
                "--watch-backend",
                "auto",
                "--poll-ms",
                "10",
                "--debounce-ms",
                "10",
                "--max-iterations",
                "1",
                "--no-git",
                "--no-fts",
                "--no-semantic-enrichment",
            ],
            &mut output,
        )
        .unwrap();
        String::from_utf8(output).unwrap()
    });
    std::thread::sleep(Duration::from_millis(50));
    fs::write(root.join("created.py"), "def created():\n    return 1\n").unwrap();
    let text = handle.join().unwrap();

    assert!(text.contains("watch event=fallback backend=poll reason=probe_timeout"));
    assert!(text.contains("watch event=refreshed backend=poll"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn watch_backend_parser_accepts_native_without_fallback() {
    let options =
        WatchOptions::parse(&["--watch-backend".to_string(), "native".to_string()]).unwrap();

    assert_eq!(options.backend, WatchBackend::Native);
}

#[test]
fn watch_once_runs_single_refresh_and_exits() {
    let root = unique_temp_dir("codebase-graph-rust-watch-once");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "watch",
            "--source-root",
            root.to_str().unwrap(),
            "--once",
            "--no-git",
            "--no-fts",
            "--no-semantic-enrichment",
        ],
        &mut output,
    )
    .unwrap();
    let text = String::from_utf8(output).unwrap();

    assert!(text.contains("watch event=refreshed event_count=0 changed_paths=0"));
    assert!(root.join(".codebaseGraph").join("manifest.json").exists());
    let _ = fs::remove_dir_all(root);
}
