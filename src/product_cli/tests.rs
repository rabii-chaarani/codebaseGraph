use super::*;

#[test]
fn prints_top_level_help() {
    let mut output = Vec::new();
    run(["--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph native CLI"));
    assert!(text.contains("materialize"));
}

#[test]
fn prints_top_level_help_without_args() {
    let mut output = Vec::new();
    run(std::iter::empty::<&str>(), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph native CLI"));
    assert!(text.contains("mcp"));
}

#[test]
fn materialize_help_is_product_command_help() {
    let mut output = Vec::new();
    run(["materialize", "--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph materialize"));
    assert!(text.contains("--native-request"));
    assert!(text.contains("local_only only"));
    assert!(!text.contains("opportunistic"));
    assert!(!text.contains("provider_first"));
}

#[test]
fn setup_help_is_product_command_help() {
    let mut output = Vec::new();
    run(["setup", "--help"], &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("codebase-graph setup"));
    assert!(text.contains("--mcp-client"));
    assert!(text.contains("local_only only"));
    assert!(!text.contains("opportunistic"));
    assert!(!text.contains("provider_first"));
}

#[test]
fn materialize_rejects_provider_backed_semantic_modes() {
    let args = vec![
        "--semantic-provider-mode".to_string(),
        "provider_first".to_string(),
    ];
    let error = MaterializeOptions::parse(&args).unwrap_err();

    assert!(error.contains("--semantic-provider-mode must be local_only"));
}

#[test]
fn setup_rejects_provider_backed_semantic_modes() {
    let args = vec![
        "--semantic-provider-mode".to_string(),
        "opportunistic".to_string(),
    ];
    let error = SetupOptions::parse(&args).unwrap_err();

    assert!(error.contains("--semantic-provider-mode must be local_only"));
}

#[test]
fn graph_schema_outputs_block_and_json() {
    let mut block = Vec::new();
    run(["graph-schema"], &mut block).unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("schema "));
    assert!(block_text.contains("helpers=8"));
    assert!(!block_text.trim_start().starts_with('{'));

    let mut json_output = Vec::new();
    run(["graph-schema", "--json"], &mut json_output).unwrap();
    let json_text = String::from_utf8(json_output).unwrap();
    assert!(!json_text.contains("\n  "));
    let value: serde_json::Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(value["ontology"], "code_ontology_v1");
    assert!(value["context_profiles"].is_object());
}

#[test]
fn graph_query_helpers_outputs_helper_catalog() {
    let mut block = Vec::new();
    run(["graph-query-helpers"], &mut block).unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("query_helpers count=8"));
    assert!(block_text.contains("repository_overview"));

    let mut json_output = Vec::new();
    run(["graph-query-helpers", "--json"], &mut json_output).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
    assert!(value["query_helpers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|helper| helper["name"] == "repository_overview"));
}

#[test]
fn graph_architecture_queries_filters_by_group() {
    let mut block = Vec::new();
    run(
        [
            "graph-architecture-queries",
            "--group",
            "overview",
            "--format",
            "block",
        ],
        &mut block,
    )
    .unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("architecture_queries "));
    assert!(block_text.contains("group overview "));
    assert!(!block_text.contains("group public_surface "));

    let mut json_output = Vec::new();
    run(
        [
            "graph-architecture-queries",
            "--group",
            "overview",
            "--json",
        ],
        &mut json_output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
    assert_eq!(value["execution_tool"], "graph_query");
    assert_eq!(value["groups"].as_array().unwrap().len(), 1);
    assert_eq!(value["groups"][0]["name"], "overview");
}

#[test]
fn graph_search_reads_native_fts_indexes() {
    let root = unique_temp_dir("codebase-graph-rust-search");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();

    setup_search_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-search",
            "SampleService",
            "--repo-root",
            root.to_str().unwrap(),
            "--limit",
            "3",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["query"], "SampleService");
    assert!(value["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hit| hit["label"] == "SampleService"));

    let mut top_output = Vec::new();
    run(
        [
            "graph-search",
            "SampleService",
            "--repo-root",
            root.to_str().unwrap(),
            "--limit",
            "1",
            "--json",
        ],
        &mut top_output,
    )
    .unwrap();
    let top_value: serde_json::Value = serde_json::from_slice(&top_output).unwrap();
    assert_eq!(top_value["results"][0]["type"], "Class");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn setup_indexes_documented_language_defaults() {
    let root = unique_temp_dir("codebase-graph-language-defaults");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub struct RustService;\nimpl RustService { pub fn run(&self) {} }\npub fn rust_helper() { RustService.run(); }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/main.go"),
        "package main\nimport \"fmt\"\nfunc GoHelper() { fmt.Println(\"ok\") }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/service.c"),
        "#include <stdio.h>\nstruct CService { int id; };\nint c_helper() { printf(\"ok\"); return 1; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/service.cpp"),
        "#include <iostream>\nclass CppService { public: void run() { cpp_helper(); } };\nint cpp_helper() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/solver.f90"),
        "module fortran_service\ncontains\nsubroutine fortran_helper()\nuse iso_fortran_env\ncall run()\nend subroutine fortran_helper\nend module fortran_service\n",
    )
    .unwrap();

    let mut setup_output = Vec::new();
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut setup_output,
    )
    .unwrap();
    let setup_value: serde_json::Value = serde_json::from_slice(&setup_output).unwrap();
    assert_eq!(setup_value["ok"], true);
    let diagnostics = setup_value["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .as_str()
            .unwrap()
            .contains("Skipped unsupported file: src/")),
        "supported language files should not be skipped: {diagnostics:?}"
    );

    let manifest_text = fs::read_to_string(root.join(".codebaseGraph/manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    for path in [
        "src/lib.rs",
        "src/main.go",
        "src/service.c",
        "src/service.cpp",
        "src/solver.f90",
    ] {
        assert!(
            manifest["files"].get(path).is_some(),
            "{path} should be materialized"
        );
    }

    for symbol in [
        "RustService",
        "GoHelper",
        "CService",
        "CppService",
        "fortran_service",
    ] {
        let mut search_output = Vec::new();
        run(
            [
                "graph-search",
                symbol,
                "--repo-root",
                root.to_str().unwrap(),
                "--limit",
                "5",
                "--json",
            ],
            &mut search_output,
        )
        .unwrap();
        let search_value: serde_json::Value = serde_json::from_slice(&search_output).unwrap();
        assert!(
            search_value["results"]
                .as_array()
                .unwrap()
                .iter()
                .any(|hit| hit["label"] == symbol),
            "{symbol} should be searchable: {search_value}"
        );
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_search_default_output_is_block() {
    let root = unique_temp_dir("codebase-graph-rust-search-block");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    setup_search_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-search",
            "helper",
            "--repo-root",
            root.to_str().unwrap(),
        ],
        &mut output,
    )
    .unwrap();

    let text = String::from_utf8(output).unwrap();
    assert!(text.starts_with("q helper\n"));
    assert!(text.contains("file path "));
    assert!(!text.trim_start().starts_with('{'));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_context_explicit_node_reads_neighbors() {
    let root = unique_temp_dir("codebase-graph-rust-context");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();

    setup_search_fixture_repo(&root);
    let mut search_output = Vec::new();
    run(
        [
            "graph-search",
            "SampleService",
            "--repo-root",
            root.to_str().unwrap(),
            "--limit",
            "1",
            "--json",
        ],
        &mut search_output,
    )
    .unwrap();
    let search: serde_json::Value = serde_json::from_slice(&search_output).unwrap();
    let hit = &search["results"][0];
    let node_id = hit["id"].as_str().unwrap();
    let node_type = hit["type"].as_str().unwrap();

    let mut output = Vec::new();
    run(
        [
            "graph-context",
            "--node-id",
            node_id,
            "--node-type",
            node_type,
            "--repo-root",
            root.to_str().unwrap(),
            "--profile",
            "brief",
            "--context-limit",
            "5",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["node_id"], node_id);
    assert_eq!(value["node_type"], node_type);
    assert!(value["context"].as_array().unwrap().iter().any(|context| {
        context["relation"] == "Contains" && context["label"].as_str().unwrap_or("") == "helper"
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_context_query_mode_uses_search_payload() {
    let root = unique_temp_dir("codebase-graph-rust-context-query");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    setup_search_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-context",
            "helper",
            "--repo-root",
            root.to_str().unwrap(),
            "--limit",
            "1",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["query"], "helper");
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_empty_project_from_native_request() {
    let root = unique_temp_dir("codebase-graph-native-cli");
    fs::create_dir_all(&root).unwrap();
    let request_path = root.join("request.json");
    let manifest_path = root.join("manifest.json");
    let db_path = root.join("graph.lbug");
    let staging_dir = root.join("staging");
    fs::write(
        &request_path,
        format!(
            r#"{{
  "source_root": "{root}",
  "repository_label": "empty",
  "mode": "full",
  "parser_version": "native-test",
  "manifest_schema_version": 1,
  "ontology": "code_ontology_v1",
  "previous_manifest": null,
  "profiles": [],
  "excluded_parts": [],
  "db_path": "{db}",
  "include_fts": false,
  "semantic_enrichment": false,
  "semantic_provider_mode": "local_only",
  "schema_statements": [],
  "staging_dir": "{staging}",
  "atomic_rebuild": true,
  "strict": true
}}"#,
            root = json_path(&root),
            db = json_path(&db_path),
            staging = json_path(&staging_dir),
        ),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--native-request",
            request_path.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["skipped"], true);
    assert!(manifest_path.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_python_source_root_without_python_request() {
    let root = unique_temp_dir("codebase-graph-rust-source-root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    let db_path = root.join(".codebaseGraph").join("graph.ldb");
    let manifest_path = root.join(".codebaseGraph").join("manifest.json");

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--db",
            db_path.to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--mode",
            "full",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["database_written"], true);
    assert_eq!(value["skipped"], false);
    assert!(db_path.exists());
    assert!(manifest_path.exists());
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    assert!(manifest["files"].get("service.py").is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn plan_lists_rebuild_delete_skip_and_ignore_paths() {
    let root = unique_temp_dir("codebase-graph-rust-plan");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    fs::write(root.join("old.py"), "def old():\n    return 1\n").unwrap();
    fs::write(root.join("notes.txt"), "not source\n").unwrap();
    fs::write(root.join("ignored.py"), "def ignored():\n    return 1\n").unwrap();
    fs::write(root.join(".codebaseGraphignore"), "ignored.py\n").unwrap();
    setup_fixture_repo(&root);

    fs::write(root.join("service.py"), "def helper():\n    return 2\n").unwrap();
    fs::write(root.join("new.py"), "def new():\n    return 3\n").unwrap();
    fs::remove_file(root.join("old.py")).unwrap();

    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "new.py");
    assert_json_array_contains(&value["would_rebuild"], "service.py");
    assert_json_array_contains(&value["would_delete"], "old.py");
    assert_json_array_contains(&value["would_skip"], "notes.txt");
    assert_json_array_contains(&value["ignored_paths"], "ignored.py");
    assert_eq!(value["database_written"], false);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn materialize_honors_config_excludes() {
    let root = unique_temp_dir("codebase-graph-rust-config-excludes");
    fs::create_dir_all(root.join(".codebaseGraph")).unwrap();
    fs::write(root.join("keep.py"), "def keep():\n    return 1\n").unwrap();
    fs::write(root.join("skip.py"), "def skip():\n    return 1\n").unwrap();
    fs::write(
        root.join(".codebaseGraph").join("config.json"),
        r#"{"materialization":{"exclude":["skip.py"]}}"#,
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "keep.py");
    assert_json_array_contains(&value["ignored_paths"], "skip.py");
    assert!(!json_array_contains(&value["would_rebuild"], "skip.py"));
    let _ = fs::remove_dir_all(root);
}

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

#[test]
fn git_diff_plan_scopes_to_changed_paths() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let root = unique_temp_dir("codebase-graph-rust-git-diff");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
    fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "initial",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    setup_fixture_repo(&root);
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    fs::write(root.join("a.py"), "def a():\n    return 2\n").unwrap();
    let mut output = Vec::new();
    run(
        [
            "plan",
            "--source-root",
            root.to_str().unwrap(),
            "--git-diff",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_json_array_contains(&value["would_rebuild"], "a.py");
    assert!(!json_array_contains(&value["would_rebuild"], "b.py"));
    assert!(!json_array_contains(&value["would_delete"], "b.py"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parallel_materialize_reports_progress_events() {
    let root = unique_temp_dir("codebase-graph-rust-progress");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.py"), "def a():\n    return 1\n").unwrap();
    fs::write(root.join("b.py"), "def b():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "materialize",
            "--source-root",
            root.to_str().unwrap(),
            "--no-git",
            "--parallel",
            "--progress",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["database_written"], true);
    assert!(value["progress_events"].as_array().unwrap().len() >= 2);
    assert_eq!(value["diff"]["added"][0], "a.py");
    assert_eq!(value["diff"]["added"][1], "b.py");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn setup_materializes_graph_and_writes_config() {
    let root = unique_temp_dir("codebase-graph-rust-setup");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let mut output = Vec::new();
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["database_written"], true);
    assert!(root.join(".codebaseGraph").join("config.json").exists());
    assert!(root.join(".codebaseGraph").join("manifest.json").exists());
    assert!(PathBuf::from(value["database_path"].as_str().unwrap()).exists());

    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".codebaseGraph").join("config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["schema_version"], 1);
    assert_eq!(config["mcp"]["server_name"], "codebase_graph");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_install_writes_generic_client_config() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-install");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
    let client_config = root.join("client").join("mcp.json");
    let config_path = root.join(".codebaseGraph").join("config.json");
    let mut output = Vec::new();
    run(
        [
            "mcp",
            "install",
            "--client",
            "generic",
            "--config-path",
            config_path.to_str().unwrap(),
            "--client-config-path",
            client_config.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["action"], "created");
    assert_eq!(value["method"], "file_adapter");
    let server_name = value["server_name"].as_str().unwrap();
    assert!(server_name.starts_with("codebase_graph_codebase-graph-rust-mcp-install"));
    assert!(client_config.exists());
    let client_payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
    assert_eq!(client_payload["mcpServers"][server_name]["args"][0], "mcp");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_install_reports_copilot_studio_metadata() {
    let root = unique_temp_dir("codebase-graph-rust-copilot-install");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
    let config_path = root.join(".codebaseGraph").join("config.json");
    let mut output = Vec::new();
    run(
        [
            "mcp",
            "install",
            "--client",
            "copilot-studio",
            "--config-path",
            config_path.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["action"], "reported");
    assert_eq!(value["method"], "manual_metadata");
    assert_eq!(value["payload"]["http"]["url"], "http://127.0.0.1:8765/mcp");
    assert_eq!(value["payload"]["stdio"]["type"], "stdio");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_health_reports_native_database() {
    let root = unique_temp_dir("codebase-graph-rust-health");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "graph-health",
            "--repo-root",
            root.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["database_exists"], true);
    assert_eq!(value["manifest_exists"], true);
    assert_eq!(value["graph_readable"], true);
    assert!(value["total_nodes"].as_u64().unwrap() > 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_query_reads_native_database() {
    let root = unique_temp_dir("codebase-graph-rust-query");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    setup_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-query",
            "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1",
            "--repo-root",
            root.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["statement"],
        "MATCH (n) RETURN count(n) AS total_nodes LIMIT 1"
    );
    assert_eq!(value["row_count"], 1);
    assert_eq!(value["truncated"], false);
    assert!(value["rows"][0][0].as_u64().unwrap() > 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_query_binds_json_parameters() {
    let root = unique_temp_dir("codebase-graph-rust-query-params");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    setup_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-query",
            "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
            "--repo-root",
            root.to_str().unwrap(),
            "--parameters",
            r#"{"path":"service.py"}"#,
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["row_count"], 1);
    assert_eq!(value["rows"][0][0], "service.py");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_graph_query_binds_json_parameters() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-query-params");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    setup_fixture_repo(&root);

    let options = McpServeOptions {
        repo_root: root.clone(),
        config: None,
        db: None,
        manifest: None,
    };
    let result = mcp_call_tool_result(
        "graph_query",
        &json!({
            "statement": "MATCH (n) WHERE n.path = $path RETURN n.path LIMIT 1",
            "parameters": {"path": "service.py"},
            "output_format": "json",
            "include_structured_content": true,
        }),
        &options,
    )
    .unwrap();

    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["row_count"], 1);
    assert_eq!(result["structuredContent"]["rows"][0][0], "service.py");
    let text_payload: serde_json::Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text_payload["rows"][0][0], "service.py");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_query_reports_truncation_without_materializing_all_rows() {
    let root = unique_temp_dir("codebase-graph-rust-query-limit");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "def helper():\n    return 1\n\ndef other():\n    return helper()\n",
    )
    .unwrap();

    setup_fixture_repo(&root);

    let mut output = Vec::new();
    run(
        [
            "graph-query",
            "MATCH (n) RETURN n.id AS id",
            "--repo-root",
            root.to_str().unwrap(),
            "--limit",
            "1",
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["row_count"], 1);
    assert_eq!(value["truncated"], true);
    assert!(value["rows"][0][0].as_str().is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_query_rejects_write_like_statements() {
    let error = run(
        ["graph-query", "MATCH (n) DELETE n", "--repo-root", "."],
        &mut Vec::new(),
    )
    .unwrap_err();
    assert!(error.contains("blocked keyword: DELETE"));
}

#[test]
fn mcp_stdio_serves_tools_and_tool_errors() {
    let root = unique_temp_dir("codebase-graph-rust-mcp");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();
    setup_search_fixture_repo(&root);

    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2025-11-25"},
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "graph_health",
                "arguments": {"include_structured_content": true},
            },
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "graph_search",
                "arguments": {
                    "query": "SampleService",
                    "limit": 2,
                    "output_format": "json",
                },
            },
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "graph_query",
                "arguments": {
                    "statement": "MATCH (n) DELETE n",
                    "include_structured_content": true,
                },
            },
        }),
    ];
    let input = requests
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n")
        + "\n";
    let options = McpServeOptions {
        repo_root: root.clone(),
        config: None,
        db: None,
        manifest: None,
    };
    let mut output = Vec::new();
    serve_mcp_stdio(&options, std::io::Cursor::new(input), &mut output).unwrap();
    let responses: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["result"]["protocolVersion"], "2025-11-25");
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "graph_health"));
    assert!(tools.iter().any(|tool| tool["name"] == "graph_search"));
    assert!(tools.iter().any(|tool| tool["name"] == "graph_query"));
    assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
        .get("output_format")
        .is_some()));
    assert!(tools.iter().all(|tool| tool["inputSchema"]["properties"]
        .get("include_structured_content")
        .is_some()));

    assert_eq!(responses[2]["result"]["isError"], false);
    assert_eq!(responses[2]["result"]["structuredContent"]["ok"], true);
    assert!(responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("health ok=true"));

    let search_text = responses[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let search_payload: serde_json::Value = serde_json::from_str(search_text).unwrap();
    assert!(search_payload["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|hit| hit["label"] == "SampleService"));

    assert_eq!(responses[4]["result"]["isError"], true);
    assert_eq!(
        responses[4]["result"]["structuredContent"]["error"]["tool"],
        "graph_query"
    );
    assert!(responses[4]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("error tool=graph_query type=ValueError"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_http_rejects_remote_bind_without_auth_token() {
    let error = McpHttpOptions::parse(&[
        "--host".to_string(),
        "0.0.0.0".to_string(),
        "--allow-remote".to_string(),
    ])
    .unwrap_err();
    assert!(error.contains("auth token"));

    let local_error =
        McpHttpOptions::parse(&["--host".to_string(), "0.0.0.0".to_string()]).unwrap_err();
    assert!(local_error.contains("localhost"));
}

#[test]
fn mcp_http_handles_initialize_list_call_and_protocol_errors() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-http");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "class SampleService:\n    def helper(self):\n        return 1\n",
    )
    .unwrap();
    setup_search_fixture_repo(&root);

    let options = test_http_options(root.clone(), None);
    let mut state = McpHttpState::default();
    let initialize = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("mcp-protocol-version", "2025-11-25")],
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(initialize.status, 200);
    assert_eq!(
        initialize.payload["result"]["protocolVersion"],
        "2025-11-25"
    );
    let session_id = initialize
        .headers
        .iter()
        .find(|(name, _)| name == "Mcp-Session-Id")
        .map(|(_, value)| value.clone())
        .unwrap();

    let missing_session = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("mcp-protocol-version", "2025-11-25")],
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        ),
    );
    assert_eq!(missing_session.status, 400);
    assert_eq!(missing_session.payload["error"]["code"], -32002);

    let listed = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
        ),
    );
    assert_eq!(listed.status, 200);
    assert!(listed.payload["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "graph_context"));

    let health = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "2025-11-25"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "graph_health", "arguments": {}},
            }),
        ),
    );
    assert_eq!(health.status, 200);
    assert_eq!(health.payload["result"]["isError"], false);
    assert!(health.payload["result"]["structuredContent"].is_null());
    assert!(health.payload["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .starts_with("health ok=true "));

    let protocol_error = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("mcp-protocol-version", "1900-01-01"),
                ("mcp-session-id", session_id.as_str()),
            ],
            json!({"jsonrpc": "2.0", "id": 5, "method": "ping", "params": {}}),
        ),
    );
    assert_eq!(protocol_error.status, 400);
    assert_eq!(protocol_error.payload["error"]["code"], -32602);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn mcp_http_enforces_bearer_token_when_configured() {
    let root = unique_temp_dir("codebase-graph-rust-mcp-http-auth");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();
    setup_search_fixture_repo(&root);

    let options = test_http_options(root.clone(), Some("secret"));
    let mut state = McpHttpState::default();
    let missing = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[("origin", "http://127.0.0.1:8765")],
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(missing.status, 401);

    let wrong = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("origin", "http://127.0.0.1:8765"),
                ("authorization", "Bearer wrong"),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(wrong.status, 401);

    let ok = handle_mcp_http_request(
        &options,
        &mut state,
        http_json_request(
            "POST",
            "/mcp",
            &[
                ("origin", "http://127.0.0.1:8765"),
                ("authorization", "Bearer secret"),
            ],
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "initialize",
                "params": {"protocolVersion": "2025-11-25"},
            }),
        ),
    );
    assert_eq!(ok.status, 200);
    assert_eq!(ok.payload["result"]["protocolVersion"], "2025-11-25");

    let _ = fs::remove_dir_all(root);
}

fn setup_fixture_repo(root: &Path) {
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
}

fn setup_search_fixture_repo(root: &Path) {
    run(
        [
            "setup",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut Vec::new(),
    )
    .unwrap();
}

fn test_http_options(root: PathBuf, auth_token: Option<&str>) -> McpHttpOptions {
    McpHttpOptions {
        serve: McpServeOptions {
            repo_root: root,
            config: None,
            db: None,
            manifest: None,
        },
        host: "127.0.0.1".to_string(),
        port: 8765,
        endpoint_path: "/mcp".to_string(),
        allow_remote: false,
        auth_token: auth_token.map(str::to_string),
    }
}

fn http_json_request(
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    payload: serde_json::Value,
) -> HttpRequest {
    let mut header_map = BTreeMap::new();
    for (name, value) in headers {
        header_map.insert(name.to_ascii_lowercase(), value.to_string());
    }
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers: header_map,
        body: serde_json::to_vec(&payload).unwrap(),
        body_too_large: false,
    }
}

fn assert_json_array_contains(value: &serde_json::Value, expected: &str) {
    assert!(
        json_array_contains(value, expected),
        "expected {value:?} to contain {expected}"
    );
}

fn json_array_contains(value: &serde_json::Value, expected: &str) -> bool {
    value
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str() == Some(expected))
}

fn watch_filter_for(root: &Path, extra_args: &[&str]) -> WatchEventFilter {
    fs::create_dir_all(root).unwrap();
    let source_root = root.canonicalize().unwrap();
    let mut args = vec![
        "--source-root".to_string(),
        source_root.to_string_lossy().to_string(),
        "--no-git".to_string(),
    ];
    args.extend(extra_args.iter().map(|arg| arg.to_string()));
    let options = MaterializeOptions::parse_with_command(&args, "watch").unwrap();
    WatchEventFilter::from_options(&source_root, &options).unwrap()
}

fn watch_test_event(root: &Path, kind: EventKind, paths: &[&str]) -> Event {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Event {
        kind,
        paths: paths.iter().map(|path| root.join(path)).collect(),
        attrs: Default::default(),
    }
}

struct WatchTestEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Drop for WatchTestEnvGuard {
    fn drop(&mut self) {
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
        env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
    }
}

fn watch_test_env_lock() -> WatchTestEnvGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap();
    env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_TIMEOUT_MS");
    env::remove_var("CODEBASE_GRAPH_WATCH_PROBE_SKIP_WRITE");
    WatchTestEnvGuard { _guard: guard }
}

fn set_test_env(key: &str, value: &str) {
    env::set_var(key, value);
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn unique_workspace_dir(prefix: &str) -> PathBuf {
    env::current_dir().unwrap().join(format!(
        ".{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn json_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
