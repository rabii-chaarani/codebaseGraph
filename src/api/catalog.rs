use crate::cli::constants::{ARCHITECTURE_QUERIES_JSON, GRAPH_SCHEMA_JSON, QUERY_HELPERS_JSON};

pub(crate) use crate::cli::format::schema_statements_from_copy_statements;

pub fn load_catalog(kind: &str) -> Result<serde_json::Value, String> {
    let source = match kind {
        "schema" => GRAPH_SCHEMA_JSON,
        "query-helpers" => QUERY_HELPERS_JSON,
        "architecture-queries" => ARCHITECTURE_QUERIES_JSON,
        _ => return Err(format!("unknown catalog kind: {kind}")),
    };
    serde_json::from_str(source)
        .map_err(|error| format!("failed to parse embedded catalog {kind}: {error}"))
}

pub fn filter_catalog(
    kind: &str,
    payload: &mut serde_json::Value,
    group: Option<&str>,
) -> Result<(), String> {
    match (kind, group) {
        ("architecture-queries", Some(group)) => {
            crate::cli::format::filter_architecture_group(payload, group)
        }
        (_, Some(_)) => Err(format!("catalog {kind} does not support group filtering")),
        (_, None) => Ok(()),
    }
}
