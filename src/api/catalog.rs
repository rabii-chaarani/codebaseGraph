#[path = "catalog_support.rs"]
pub(crate) mod support;

pub(crate) use self::support::schema_statements_from_copy_statements;
use self::support::{
    filter_architecture_group, metadata_payload, ARCHITECTURE_QUERIES_JSON, GRAPH_SCHEMA_JSON,
    QUERY_HELPERS_JSON,
};
use crate::parser::grammar_node_types;
use crate::profiles::built_in_profiles;
use crate::protocol::LanguageProfile;
use serde_json::{json, Value};

pub fn load_catalog(kind: &str) -> Result<Value, String> {
    let source = match kind {
        "schema" => GRAPH_SCHEMA_JSON,
        "query-helpers" => QUERY_HELPERS_JSON,
        "architecture-queries" => ARCHITECTURE_QUERIES_JSON,
        "syntax" => return Ok(json!({"languages": supported_syntax_languages()})),
        _ => return Err(format!("unknown catalog kind: {kind}")),
    };
    metadata_payload(source)
        .map_err(|error| format!("failed to parse embedded catalog {kind}: {error}"))
}

pub fn filter_catalog(kind: &str, payload: &mut Value, group: Option<&str>) -> Result<(), String> {
    match (kind, group) {
        ("architecture-queries", Some(group)) => filter_architecture_group(payload, group),
        ("syntax", Some(language)) => {
            *payload = syntax_catalog(language)?;
            Ok(())
        }
        ("syntax", None) => Err("syntax catalog requires a language".to_string()),
        (_, Some(_)) => Err(format!("catalog {kind} does not support group filtering")),
        (_, None) => Ok(()),
    }
}

pub(crate) fn supported_syntax_languages() -> Vec<String> {
    let mut languages = built_in_profiles()
        .into_iter()
        .map(|profile| profile.language)
        .collect::<Vec<_>>();
    languages.sort();
    languages
}

fn syntax_catalog(language: &str) -> Result<Value, String> {
    let profile = built_in_profiles()
        .into_iter()
        .find(|profile| profile.language == language)
        .ok_or_else(|| unsupported_syntax_language(language))?;
    let node_types = if profile.language == "markdown" {
        markdown_node_types()
    } else {
        tree_sitter_node_types(&profile)?
    };

    Ok(json!({
        "language": profile.language,
        "grammar_package": profile.grammar_package,
        "grammar_version": profile.grammar_version,
        "root_node_types": profile.root_node_types,
        "node_types": node_types,
        "capture_mappings": profile.capture_mappings,
    }))
}

fn tree_sitter_node_types(profile: &LanguageProfile) -> Result<Vec<Value>, String> {
    let source = grammar_node_types(profile)
        .ok_or_else(|| unsupported_syntax_language(&profile.language))?;
    let mut node_types = serde_json::from_str::<Vec<Value>>(source).map_err(|error| {
        format!(
            "failed to parse node metadata for syntax language {}: {error}",
            profile.language
        )
    })?;
    node_types.retain(|node| node.get("named").and_then(Value::as_bool) == Some(true));
    node_types.sort_by(|left, right| node_type_name(left).cmp(node_type_name(right)));
    Ok(node_types)
}

fn markdown_node_types() -> Vec<Value> {
    vec![
        json!({"type": "DocumentationChunk", "named": true, "fields": {}}),
        json!({"type": "DocumentationSource", "named": true, "fields": {}}),
        json!({
            "type": "Module",
            "named": true,
            "fields": {},
            "children": {
                "multiple": true,
                "required": false,
                "types": [
                    {"type": "DocumentationChunk", "named": true},
                    {"type": "DocumentationSource", "named": true}
                ]
            }
        }),
    ]
}

fn node_type_name(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or_default()
}

fn unsupported_syntax_language(language: &str) -> String {
    format!(
        "Unknown syntax language: {language}. Valid languages: {}",
        supported_syntax_languages().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::{filter_catalog, load_catalog};

    #[test]
    fn shared_ontology_and_query_catalogs_remain_available() {
        let schema = load_catalog("schema").expect("schema catalog should load");
        assert_eq!(schema["ontology"], "code_ontology_v1");

        let query_helpers =
            load_catalog("query-helpers").expect("query helper catalog should load");
        assert!(query_helpers["query_helpers"].is_array());

        let architecture_queries =
            load_catalog("architecture-queries").expect("architecture query catalog should load");
        assert!(architecture_queries["groups"].is_array());
    }

    #[test]
    fn syntax_catalog_retains_named_tree_sitter_nodes_and_fields() {
        let mut payload = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut payload, Some("rust"))
            .expect("rust syntax catalog should load");

        assert_eq!(payload["language"], "rust");
        assert_eq!(payload["grammar_version"], "tree_sitter_rust@0.24.2");
        assert!(payload["node_types"]
            .as_array()
            .expect("node types should be an array")
            .iter()
            .all(|node| node["named"] == true));
        let function = payload["node_types"]
            .as_array()
            .and_then(|nodes| nodes.iter().find(|node| node["type"] == "function_item"))
            .expect("function item should be cataloged");
        assert!(function["fields"].get("name").is_some());
        assert!(payload["capture_mappings"]
            .as_array()
            .expect("capture mappings should be an array")
            .iter()
            .any(|mapping| mapping["capture_name"] == "definition.function"));
    }

    #[test]
    fn syntax_catalog_describes_builtin_markdown_without_grammar_fields() {
        let mut payload = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut payload, Some("markdown"))
            .expect("markdown syntax catalog should load");

        assert_eq!(payload["grammar_version"], "builtin-markdown@1");
        assert_eq!(
            payload["node_types"]
                .as_array()
                .expect("node types should be an array")
                .iter()
                .map(|node| node["type"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["DocumentationChunk", "DocumentationSource", "Module"]
        );
        assert!(payload["node_types"]
            .as_array()
            .expect("node types should be an array")
            .iter()
            .all(|node| node["fields"]
                .as_object()
                .is_some_and(|fields| fields.is_empty())));
    }

    #[test]
    fn syntax_catalog_describes_css_stylesheet_nodes() {
        let mut payload = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut payload, Some("css"))
            .expect("CSS syntax catalog should load");

        assert_eq!(payload["grammar_version"], "tree_sitter_css@0.25.0");
        let nodes = payload["node_types"]
            .as_array()
            .expect("node types should be an array");
        assert!(nodes.iter().any(|node| node["type"] == "stylesheet"));
        assert!(nodes.iter().any(|node| node["type"] == "rule_set"));
        assert!(payload["capture_mappings"]
            .as_array()
            .is_some_and(Vec::is_empty));
    }

    #[test]
    fn syntax_catalog_distinguishes_typescript_and_tsx_grammars() {
        let mut typescript = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut typescript, Some("typescript"))
            .expect("TypeScript syntax catalog should load");
        assert_eq!(
            typescript["grammar_version"],
            "tree_sitter_typescript@0.23.2"
        );
        assert!(typescript["node_types"]
            .as_array()
            .is_some_and(|nodes| nodes
                .iter()
                .any(|node| node["type"] == "interface_declaration")));

        let mut tsx = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut tsx, Some("tsx")).expect("TSX syntax catalog should load");
        assert!(tsx["node_types"]
            .as_array()
            .is_some_and(|nodes| nodes.iter().any(|node| node["type"] == "jsx_element")));
    }

    #[test]
    fn syntax_catalog_describes_javascript_and_jsx_nodes() {
        let mut payload = load_catalog("syntax").expect("syntax catalog index should load");
        filter_catalog("syntax", &mut payload, Some("javascript"))
            .expect("JavaScript syntax catalog should load");

        assert_eq!(payload["grammar_version"], "tree_sitter_javascript@0.25.0");
        let nodes = payload["node_types"]
            .as_array()
            .expect("node types should be an array");
        assert!(nodes
            .iter()
            .any(|node| node["type"] == "function_declaration"));
        assert!(nodes.iter().any(|node| node["type"] == "jsx_element"));
    }

    #[test]
    fn syntax_catalog_rejects_missing_and_unsupported_languages() {
        let mut payload = load_catalog("syntax").expect("syntax catalog index should load");
        assert_eq!(
            filter_catalog("syntax", &mut payload, None).unwrap_err(),
            "syntax catalog requires a language"
        );
        assert!(filter_catalog("syntax", &mut payload, Some("custom"))
            .unwrap_err()
            .contains(
                "Valid languages: c, cpp, css, fortran, go, javascript, markdown, python, rust, tsx, typescript"
            ));
    }
}
