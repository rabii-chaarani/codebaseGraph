use super::*;

#[test]
fn graph_schema_outputs_block_and_json() {
    let mut block = Vec::new();
    run(["schema"], &mut block).unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("schema "));
    assert!(block_text.contains("helpers=8"));
    assert!(!block_text.trim_start().starts_with('{'));

    let mut json_output = Vec::new();
    run(["schema", "--json"], &mut json_output).unwrap();
    let json_text = String::from_utf8(json_output).unwrap();
    assert!(!json_text.contains("\n  "));
    let value: serde_json::Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(value["ontology"], "code_ontology_v1");
    assert!(value["context_profiles"].is_object());
}

#[test]
fn graph_syntax_outputs_language_catalog_and_validates_language() {
    let mut block = Vec::new();
    run(["syntax", "rust"], &mut block).unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("syntax language=rust"));
    assert!(block_text.contains("node function_item"));

    let mut json_output = Vec::new();
    run(["syntax", "markdown", "--json"], &mut json_output).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
    assert_eq!(value["language"], "markdown");
    assert_eq!(value["grammar_version"], "builtin-markdown@1");

    let mut css_output = Vec::new();
    run(["syntax", "css", "--json"], &mut css_output).unwrap();
    let css: serde_json::Value = serde_json::from_slice(&css_output).unwrap();
    assert_eq!(css["language"], "css");
    assert!(css["node_types"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().any(|node| node["type"] == "rule_set")));

    let mut javascript_output = Vec::new();
    run(["syntax", "javascript", "--json"], &mut javascript_output).unwrap();
    let javascript: serde_json::Value = serde_json::from_slice(&javascript_output).unwrap();
    assert_eq!(javascript["language"], "javascript");
    assert!(javascript["node_types"]
        .as_array()
        .is_some_and(|nodes| nodes.iter().any(|node| node["type"] == "jsx_element")));

    let error = run(["syntax", "custom"], &mut Vec::new()).unwrap_err();
    assert!(error.contains("Unknown syntax language"));
    assert!(run(["syntax"], &mut Vec::new())
        .unwrap_err()
        .contains("syntax requires a language"));
}

#[test]
fn graph_query_helpers_outputs_helper_catalog() {
    let mut block = Vec::new();
    run(["query-helpers"], &mut block).unwrap();
    let block_text = String::from_utf8(block).unwrap();
    assert!(block_text.starts_with("query_helpers count=8"));
    assert!(block_text.contains("repository_overview"));

    let mut json_output = Vec::new();
    run(["query-helpers", "--json"], &mut json_output).unwrap();
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
            "codebase-architecture-queries",
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
            "codebase-architecture-queries",
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
    let manifest_path = managed_active_manifest_path(&root);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let search_backend = manifest["search_backend"]
        .as_object()
        .expect("new search-enabled generations should declare their sidecar backend");
    assert_eq!(search_backend["backend"], "disk_bm25_v1");
    let database_path = manifest_path.with_file_name("graph.ldb");
    for suffix in search_backend["files"].as_object().unwrap().keys() {
        assert!(PathBuf::from(format!("{}.{}", database_path.display(), suffix)).is_file());
    }

    let mut output = Vec::new();
    run(
        [
            "codebase-search",
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
            "codebase-search",
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
fn graph_layers_search_syntax_and_preserve_hybrid_context_structure() {
    let root = unique_temp_dir("codebase-graph-layered-search");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("service.py"),
        "def helper(value):\n    return value\n",
    )
    .unwrap();
    setup_search_fixture_repo(&root);

    let mut default_output = Vec::new();
    run(
        [
            "codebase-search",
            "function_definition",
            "--repo-root",
            root.to_str().unwrap(),
            "--json",
        ],
        &mut default_output,
    )
    .unwrap();
    let default_value: serde_json::Value = serde_json::from_slice(&default_output).unwrap();
    assert_eq!(default_value["layer"], "semantic");
    assert!(default_value["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|result| result["type"] != "SyntaxCapture"));

    let mut syntax_output = Vec::new();
    run(
        [
            "codebase-search",
            "function_definition",
            "--repo-root",
            root.to_str().unwrap(),
            "--layer",
            "syntax",
            "--limit",
            "10",
            "--json",
        ],
        &mut syntax_output,
    )
    .unwrap();
    let syntax_value: serde_json::Value = serde_json::from_slice(&syntax_output).unwrap();
    assert_eq!(syntax_value["layer"], "syntax");
    let syntax_results = syntax_value["results"].as_array().unwrap();
    assert!(syntax_results.iter().all(|result| {
        result["type"] == "SyntaxCapture"
            && result["layer"] == "syntax"
            && result["grammar_version"] == "tree_sitter_python@0.25.0"
    }));
    let function = syntax_results
        .iter()
        .find(|result| result["tree_sitter_node_type"] == "function_definition")
        .expect("function syntax node should be searchable");

    let mut hybrid_output = Vec::new();
    run(
        [
            "codebase-search",
            "helper",
            "--repo-root",
            root.to_str().unwrap(),
            "--layer",
            "hybrid",
            "--limit",
            "20",
            "--json",
        ],
        &mut hybrid_output,
    )
    .unwrap();
    let hybrid_value: serde_json::Value = serde_json::from_slice(&hybrid_output).unwrap();
    assert_eq!(hybrid_value["layer"], "hybrid");
    let hybrid_results = hybrid_value["results"].as_array().unwrap();
    let first_syntax = hybrid_results
        .iter()
        .position(|result| result["layer"] == "syntax")
        .expect("hybrid results should include syntax matches");
    assert!(first_syntax > 0);
    assert!(hybrid_results[..first_syntax]
        .iter()
        .all(|result| result["layer"] == "semantic"));

    let mut syntax_context_output = Vec::new();
    run(
        [
            "codebase-context",
            "--node-id",
            function["id"].as_str().unwrap(),
            "--node-type",
            "SyntaxCapture",
            "--repo-root",
            root.to_str().unwrap(),
            "--layer",
            "syntax",
            "--context-limit",
            "10",
            "--json",
        ],
        &mut syntax_context_output,
    )
    .unwrap();
    let syntax_context: serde_json::Value = serde_json::from_slice(&syntax_context_output).unwrap();
    assert_eq!(syntax_context["layer"], "syntax");
    let syntax_children = syntax_context["context"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|context| context["relation"] == "SyntaxChild")
        .collect::<Vec<_>>();
    assert!(!syntax_children.is_empty());
    assert!(syntax_children
        .iter()
        .all(|context| context["child_index"].is_i64()));
    assert!(syntax_children
        .windows(2)
        .all(|pair| pair[0]["child_index"].as_i64() <= pair[1]["child_index"].as_i64()));

    let mut hybrid_context_output = Vec::new();
    run(
        [
            "codebase-context",
            "--node-id",
            function["id"].as_str().unwrap(),
            "--node-type",
            "SyntaxCapture",
            "--repo-root",
            root.to_str().unwrap(),
            "--layer",
            "hybrid",
            "--context-limit",
            "10",
            "--json",
        ],
        &mut hybrid_context_output,
    )
    .unwrap();
    let hybrid_context: serde_json::Value = serde_json::from_slice(&hybrid_context_output).unwrap();
    assert_eq!(hybrid_context["layer"], "hybrid");
    assert!(hybrid_context["context"]
        .as_array()
        .unwrap()
        .iter()
        .any(|context| {
            matches!(
                context["relation"].as_str(),
                Some("SyntaxChild" | "DerivedFrom" | "EvidencedBy")
            )
        }));

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
            "install",
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

    let manifest_text = fs::read_to_string(managed_active_manifest_path(&root)).unwrap();
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
                "codebase-search",
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
            "codebase-search",
            "helper",
            "--repo-root",
            root.to_str().unwrap(),
        ],
        &mut output,
    )
    .unwrap();

    let text = String::from_utf8(output).unwrap();
    assert!(text.starts_with("q helper layer=semantic\n"));
    assert!(text.contains("file path "));
    assert!(!text.trim_start().starts_with('{'));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_layer_help_documents_supported_values() {
    let mut search_help = Vec::new();
    run(["codebase-search", "--help"], &mut search_help).unwrap();
    assert!(String::from_utf8(search_help)
        .unwrap()
        .contains("--layer semantic|syntax|hybrid"));

    let mut context_help = Vec::new();
    run(["codebase-context", "--help"], &mut context_help).unwrap();
    assert!(String::from_utf8(context_help)
        .unwrap()
        .contains("--layer semantic|syntax|hybrid"));
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
            "codebase-search",
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
            "codebase-context",
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
            "codebase-context",
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
    assert_eq!(value["layer"], "semantic");
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_health_reports_native_database() {
    let root = unique_temp_dir("codebase-graph-rust-health");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    run(
        [
            "install",
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
            "check-health",
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
    assert_eq!(value["storage_format"], "managed_v2");
    assert_eq!(value["writable"], true);
    assert!(value["active_generation"].as_str().is_some());
    assert_eq!(value["pending_runs"], 0);
    assert_eq!(value["cleanup_pending"], false);
    assert!(value["physical_database_bytes"].as_u64().unwrap() > 0);
    assert!(value["logical_database_bytes"].as_u64().unwrap() > 0);
    assert!(
        value["physical_database_bytes"].as_u64().unwrap()
            >= value["logical_database_bytes"].as_u64().unwrap()
    );
    assert_eq!(value["remediation"], serde_json::Value::Null);
    assert!(value["total_nodes"].as_u64().unwrap() > 0);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_health_keeps_legacy_storage_readable_but_requires_reinstall_for_writes() {
    let root = unique_temp_dir("codebase-graph-rust-health-legacy");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("service.py"), "def helper():\n    return 1\n").unwrap();

    let mut install_output = Vec::new();
    run(
        [
            "install",
            "--repo-root",
            root.to_str().unwrap(),
            "--mode",
            "full",
            "--mcp-client",
            "none",
            "--instructions-target",
            "skip",
            "--no-fts",
            "--no-semantic-enrichment",
            "--json",
        ],
        &mut install_output,
    )
    .unwrap();
    let install_value: serde_json::Value = serde_json::from_slice(&install_output).unwrap();
    let state = root.join(".codebaseGraph");
    fs::write(
        state.join("config.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "repo_root": root,
            "database_path": install_value["db_path"],
            "manifest_path": install_value["manifest_path"],
        }))
        .unwrap(),
    )
    .unwrap();

    let mut output = Vec::new();
    run(
        [
            "check-health",
            "--repo-root",
            root.to_str().unwrap(),
            "--json",
        ],
        &mut output,
    )
    .unwrap();

    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["storage_format"], "legacy_v1");
    assert_eq!(value["writable"], false);
    assert_eq!(value["graph_readable"], true);
    assert!(value["physical_database_bytes"].as_u64().unwrap() > 0);
    assert!(
        value["physical_database_bytes"].as_u64().unwrap()
            >= value["logical_database_bytes"].as_u64().unwrap()
    );
    assert!(value["remediation"]
        .as_str()
        .unwrap()
        .contains("codebase-graph reinstall --repo-root"));
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
