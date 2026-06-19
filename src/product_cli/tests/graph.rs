use super::*;

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
