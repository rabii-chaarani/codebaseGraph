#[test]
fn api_does_not_import_transport_adapters() {
    let api_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/api");
    let mut pending = vec![api_root];
    let forbidden = ["crate", "::cli"].concat();

    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).expect("API directory should be readable") {
            let path = entry
                .expect("API directory entry should be readable")
                .path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs")
                || path.file_name().and_then(std::ffi::OsStr::to_str) == Some("boundary_tests.rs")
            {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("API source should be readable");
            assert!(
                !source.contains(&forbidden),
                "{} imports a CLI transport module",
                path.display()
            );
        }
    }
}

#[test]
fn transport_adapters_do_not_import_product_execution_services() {
    let adapters = [
        ("CLI dispatch", include_str!("../cli/dispatch.rs")),
        (
            "CLI materialization",
            include_str!("../cli/materialization/command.rs"),
        ),
        ("MCP tools", include_str!("../cli/mcp/tools.rs")),
        ("watch command", include_str!("../cli/watch/command.rs")),
        ("MCP refresh", include_str!("../cli/mcp/refresh.rs")),
    ];
    let forbidden = [
        ["crate", "::db_writer"].concat(),
        ["crate", "::scan"].concat(),
        ["crate", "::execution"].concat(),
        ["crate", "::semantic_enrichment"].concat(),
        ["crate", "::staging_writer"].concat(),
        ["crate", "::cli::graph"].concat(),
        ["crate", "::cli::materialization"].concat(),
        ["materialize", "_syntax_batch"].concat(),
        ["execute", "_materialization_pipeline"].concat(),
        ["execute", "_graph_search"].concat(),
        ["execute", "_read_only_query"].concat(),
        ["execute", "_refresh_operation"].concat(),
        ["start", "_native_watcher"].concat(),
        ["run", "_poll_watch"].concat(),
        ["resolve", "_source_root"].concat(),
        ["write", "_database("].concat(),
    ];

    for (name, source) in adapters {
        for dependency in &forbidden {
            assert!(
                !source.contains(dependency),
                "{name} bypasses the Public API Facade through {dependency}"
            );
        }
    }
}

#[test]
fn only_graph_writer_submits_database_updates() {
    let graph_writer = include_str!("../staging_writer/writer.rs");
    assert!(graph_writer.contains(&["write", "_database("].concat()));

    let other_materialization_modules = [
        include_str!("../execution/run.rs"),
        include_str!("../execution/plan.rs"),
        include_str!("../execution/parallel.rs"),
        include_str!("../semantic_enrichment/mod.rs"),
        include_str!("../cli/materialization/command.rs"),
        include_str!("materialization.rs"),
    ];
    for source in other_materialization_modules {
        assert!(!source.contains(&["write", "_database("].concat()));
    }
}
