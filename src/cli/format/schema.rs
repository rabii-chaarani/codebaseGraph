use super::{metadata_payload, value_array, value_str};
use crate::cli::{constants::GRAPH_SCHEMA_JSON, graph::cypher_single_quoted};
use std::collections::BTreeSet;

pub(in crate::cli) fn schema_statements_from_copy_statements(
    include_fts: bool,
    copy_statements: &[String],
) -> Vec<String> {
    let tables = copy_tables(copy_statements);
    let relation_names = relation_names(&tables);
    let mut node_tables: Vec<String> = tables
        .iter()
        .filter(|table| {
            !table.starts_with("FROM_")
                && !table.starts_with("TO_")
                && !relation_names.contains(*table)
        })
        .cloned()
        .collect();
    let mut relation_tables: Vec<String> = relation_names.into_iter().collect();
    node_tables.sort();
    relation_tables.sort();

    let mut statements = vec!["INSTALL json".to_string(), "LOAD json".to_string()];
    if include_fts {
        statements.extend(["INSTALL fts".to_string(), "LOAD fts".to_string()]);
    }
    statements.extend(
        node_tables
            .iter()
            .map(|table| node_table_sql(table, node_fields(table))),
    );
    statements.extend(
        relation_tables
            .iter()
            .map(|table| node_table_sql(table, edge_fields())),
    );
    for relation in &relation_tables {
        statements.push(relation_table_sql(
            &format!("FROM_{relation}"),
            &node_tables,
            &[relation.to_string()],
            "source",
        ));
        statements.push(relation_table_sql(
            &format!("TO_{relation}"),
            &[relation.to_string()],
            &node_tables,
            "target",
        ));
    }
    if include_fts {
        statements.extend(fts_index_statements(&node_tables));
    }
    statements
}

pub(in crate::cli) fn fts_index_statements(node_tables: &[String]) -> Vec<String> {
    let Ok(schema) = metadata_payload(GRAPH_SCHEMA_JSON) else {
        return Vec::new();
    };
    let present_tables: BTreeSet<&str> = node_tables.iter().map(String::as_str).collect();
    let mut statements = Vec::new();
    for index in value_array(&schema, "search_indexes") {
        let index_name = value_str(index, "name");
        let fields = index
            .get("fields")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(|field| format!("'{}'", cypher_single_quoted(field)))
            .collect::<Vec<_>>()
            .join(", ");
        for node_type in index
            .get("node_types")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|node_type| present_tables.contains(*node_type))
        {
            statements.push(format!(
                "CALL CREATE_FTS_INDEX('{}', '{}_{}', [{}])",
                cypher_single_quoted(node_type),
                cypher_single_quoted(index_name),
                cypher_single_quoted(node_type),
                fields
            ));
        }
    }
    statements
}

pub(in crate::cli) fn copy_tables(copy_statements: &[String]) -> BTreeSet<String> {
    copy_statements
        .iter()
        .filter_map(|statement| {
            let start = statement.find('`')?;
            let rest = &statement[start + 1..];
            let end = rest.find('`')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

pub(in crate::cli) fn relation_names(tables: &BTreeSet<String>) -> BTreeSet<String> {
    let mut relations = BTreeSet::new();
    for table in tables {
        if let Some(name) = table.strip_prefix("FROM_") {
            relations.insert(name.to_string());
        }
        if let Some(name) = table.strip_prefix("TO_") {
            relations.insert(name.to_string());
        }
    }
    relations
}

pub(in crate::cli) fn node_table_sql(
    table: &str,
    fields: Vec<(&'static str, &'static str)>,
) -> String {
    let columns: Vec<String> = fields
        .into_iter()
        .map(|(name, value_type)| {
            let primary_key = if name == "id" { " PRIMARY KEY" } else { "" };
            format!("  `{name}` {value_type}{primary_key}")
        })
        .collect();
    format!(
        "CREATE NODE TABLE IF NOT EXISTS `{table}`(\n{}\n)",
        columns.join(",\n")
    )
}

pub(in crate::cli) fn relation_table_sql(
    table: &str,
    from_tables: &[String],
    to_tables: &[String],
    role: &str,
) -> String {
    let endpoints: Vec<String> = from_tables
        .iter()
        .flat_map(|from_table| {
            to_tables
                .iter()
                .map(move |to_table| format!("  FROM `{from_table}` TO `{to_table}`"))
        })
        .collect();
    let mut columns = endpoints;
    columns.push(format!("  `role` STRING DEFAULT '{role}'"));
    format!(
        "CREATE REL TABLE IF NOT EXISTS `{table}`(\n{}\n)",
        columns.join(",\n")
    )
}

pub(in crate::cli) fn node_fields(table: &str) -> Vec<(&'static str, &'static str)> {
    let mut fields = common_node_fields();
    if table == "File" {
        fields.push(("content_hash", "STRING"));
    }
    fields
}

pub(in crate::cli) fn common_node_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "STRING"),
        ("label", "STRING"),
        ("kind", "STRING"),
        ("language", "STRING"),
        ("path", "STRING"),
        ("qualified_name", "STRING"),
        ("scope_id", "STRING"),
        ("line_start", "INT64"),
        ("line_end", "INT64"),
        ("byte_start", "INT64"),
        ("byte_end", "INT64"),
        ("tree_sitter_node_type", "STRING"),
        ("capture_name", "STRING"),
        ("summary", "STRING"),
        ("metadata", "JSON"),
    ]
}

pub(in crate::cli) fn edge_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("id", "STRING"),
        ("kind", "STRING"),
        ("source_id", "STRING"),
        ("target_id", "STRING"),
        ("confidence", "DOUBLE"),
        ("line_start", "INT64"),
        ("line_end", "INT64"),
        ("byte_start", "INT64"),
        ("byte_end", "INT64"),
        ("metadata", "JSON"),
    ]
}
