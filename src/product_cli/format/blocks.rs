pub(in crate::product_cli) fn serialize_schema_block(payload: &serde_json::Value) -> String {
    let node_types = value_array(payload, "node_types");
    let relation_types = value_array(payload, "relation_types");
    let parser_mappings = value_array(payload, "parser_node_mappings");
    let search_indexes = value_array(payload, "search_indexes");
    let profiles = payload
        .get("context_profiles")
        .and_then(serde_json::Value::as_object)
        .map(serde_json::Map::len)
        .unwrap_or_default();
    let query_helpers = value_array(payload, "query_helpers");
    let mut lines = vec![format!(
        "schema {} version={} nodes={} relations={} parser_mappings={} indexes={} profiles={} helpers={}",
        block_value(value_str(payload, "ontology")),
        block_value(value_str(payload, "version")),
        node_types.len(),
        relation_types.len(),
        parser_mappings.len(),
        search_indexes.len(),
        profiles,
        query_helpers.len()
    )];
    if !node_types.is_empty() {
        lines.push(format!("node_types {}", csv_names(node_types)));
    }
    if !relation_types.is_empty() {
        lines.push(format!("relation_types {}", csv_names(relation_types)));
    }
    for index in search_indexes {
        lines.push(format!(
            "index {} node_types={} fields={}",
            block_value(value_str(index, "name")),
            csv_values(index.get("node_types")),
            csv_values(index.get("fields"))
        ));
    }
    if let Some(context_profiles) = payload
        .get("context_profiles")
        .and_then(serde_json::Value::as_object)
    {
        for (name, profile) in context_profiles {
            let relations = csv_values(profile.get("relations"));
            if relations.is_empty() {
                lines.push(format!("profile {}", block_value(name)));
            } else {
                lines.push(format!(
                    "profile {} relations={relations}",
                    block_value(name)
                ));
            }
        }
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn serialize_query_helpers_block(payload: &serde_json::Value) -> String {
    let helpers = value_array(payload, "query_helpers");
    let mut lines = vec![format!("query_helpers count={}", helpers.len())];
    for helper in helpers {
        append_query_spec_lines(&mut lines, helper, "");
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn serialize_architecture_queries_block(
    payload: &serde_json::Value,
) -> String {
    let groups = value_array(payload, "groups");
    let mut lines = vec![format!(
        "architecture_queries workflow={} execution_tool={} groups={}",
        block_value(value_str(payload, "workflow")),
        block_value(value_str(payload, "execution_tool")),
        groups.len()
    )];
    let recommended_order = csv_values(payload.get("recommended_order"));
    if !recommended_order.is_empty() {
        lines.push(format!("recommended_order {recommended_order}"));
    }
    for group in groups {
        lines.push(format!(
            "group {} goal={}",
            block_value(value_str(group, "name")),
            block_value(value_str(group, "goal"))
        ));
        for query in value_array(group, "queries") {
            append_query_spec_lines(&mut lines, query, "  ");
        }
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn serialize_health_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "health ok={} database_exists={} manifest_exists={} graph_readable={} total_nodes={}",
        value_bool(payload, "ok"),
        value_bool(payload, "database_exists"),
        value_bool(payload, "manifest_exists"),
        value_bool(payload, "graph_readable"),
        payload
            .get("total_nodes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
    )];
    for key in ["repo_root", "database_path", "manifest_path"] {
        lines.push(format!("{key} {}", block_value(value_str(payload, key))));
    }
    if let Some(error) = payload.get("error").and_then(serde_json::Value::as_str) {
        lines.push(format!("error {}", block_value(error)));
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn serialize_search_block(payload: &serde_json::Value) -> String {
    let results = value_array(payload, "results");
    let mut lines = vec![format!("q {}", block_value(value_str(payload, "query")))];
    let mut current_path: Option<String> = None;
    for result in results {
        let result_path = value_str(result, "path").to_string();
        if current_path.as_deref() != Some(result_path.as_str()) {
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(format!("file path {}", block_value(&result_path)));
            current_path = Some(result_path);
        }
        let mut result_parts = vec![
            format!("- {}", value_str(result, "type")),
            block_value(value_str(result, "label")),
            block_span(result.get("span")),
        ];
        if let Some(rank_score) = result.get("rank_score").and_then(serde_json::Value::as_f64) {
            result_parts.push(format!("rank_score={rank_score:.2}"));
        }
        if let Some(id) = result.get("id").and_then(serde_json::Value::as_str) {
            result_parts.push(format!("id={}", block_value(id)));
        }
        let summary = value_str(result, "summary");
        if !summary.is_empty() && summary != value_str(result, "label") {
            result_parts.push(format!("summary={}", block_value(summary)));
        }
        lines.push(result_parts.join(" "));
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn serialize_query_block(payload: &serde_json::Value) -> String {
    let rows = value_array(payload, "rows");
    let columns = query_columns(value_str(payload, "statement"));
    let mut lines = vec![format!(
        "query rows={} truncated={}",
        payload
            .get("row_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(rows.len() as u64),
        value_bool(payload, "truncated")
    )];
    lines.push(format!(
        "statement {}",
        block_value(value_str(payload, "statement"))
    ));
    if !columns.is_empty() {
        lines.push(format!("columns {}", columns.join(",")));
    }
    for (index, row) in rows.iter().enumerate() {
        let values = row
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![(*row).clone()]);
        let row_text = if !columns.is_empty() && columns.len() == values.len() {
            columns
                .iter()
                .zip(values.iter())
                .map(|(column, value)| format!("{column}={}", block_json_value(value)))
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            values
                .iter()
                .map(block_json_value)
                .collect::<Vec<_>>()
                .join(" ")
        };
        lines.push(
            format!("row {} {}", index + 1, row_text)
                .trim_end()
                .to_string(),
        );
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn query_columns(statement: &str) -> Vec<String> {
    let upper = statement.to_ascii_uppercase();
    let Some(return_index) = upper.find("RETURN") else {
        return Vec::new();
    };
    let after_return = return_index + "RETURN".len();
    let end = ["ORDER BY", "LIMIT"]
        .iter()
        .filter_map(|marker| {
            upper[after_return..]
                .find(marker)
                .map(|index| after_return + index)
        })
        .min()
        .unwrap_or(statement.len());
    split_return_expressions(&statement[after_return..end])
        .into_iter()
        .filter_map(|expression| query_column_label(&expression))
        .collect()
}

pub(in crate::product_cli) fn split_return_expressions(text: &str) -> Vec<String> {
    let mut expressions = Vec::new();
    let mut current = String::new();
    let mut depth = 0_i32;
    for character in text.chars() {
        match character {
            '(' => {
                depth += 1;
                current.push(character);
            }
            ')' => {
                depth = (depth - 1).max(0);
                current.push(character);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    expressions.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        expressions.push(trimmed.to_string());
    }
    expressions
}

pub(in crate::product_cli) fn query_column_label(expression: &str) -> Option<String> {
    let parts = expression.split_whitespace().collect::<Vec<_>>();
    for index in 0..parts.len().saturating_sub(1) {
        if parts[index].eq_ignore_ascii_case("AS") && is_identifier(parts[index + 1]) {
            return Some(parts[index + 1].to_string());
        }
    }
    let label = expression.rsplit('.').next()?.trim();
    if is_identifier(label) {
        Some(label.to_string())
    } else {
        None
    }
}

pub(in crate::product_cli) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(in crate::product_cli) fn block_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => block_value(value),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
    }
}

pub(in crate::product_cli) fn serialize_error_block(payload: &serde_json::Value) -> String {
    let error = payload.get("error").unwrap_or(payload);
    format!(
        "error tool={} type={} message={}\n",
        block_value(value_str(error, "tool")),
        block_value(value_str(error, "type")),
        block_value(value_str(error, "message"))
    )
}

pub(in crate::product_cli) fn serialize_context_block(payload: &serde_json::Value) -> String {
    let mut lines = vec![format!(
        "context {} id={} profile={}",
        value_str(payload, "node_type"),
        block_value(value_str(payload, "node_id")),
        block_value(value_str(payload, "profile"))
    )];
    let mut current_path: Option<String> = None;
    for context in value_array(payload, "context") {
        let context_path = value_str(context, "path").to_string();
        if current_path.as_deref() != Some(context_path.as_str()) {
            if lines.len() > 1 {
                lines.push(String::new());
            }
            lines.push(format!("file path {}", block_value(&context_path)));
            current_path = Some(context_path);
        }
        let mut parts = vec![
            value_str(context, "direction").to_string(),
            value_str(context, "relation").to_string(),
            value_str(context, "type").to_string(),
            block_value(value_str(context, "label")),
            block_span(context.get("span")),
        ];
        let summary = value_str(context, "summary");
        if !summary.is_empty() && summary != value_str(context, "label") {
            parts.push(format!("summary={}", block_value(summary)));
        }
        lines.push(parts.join(" ").trim_end().to_string());
    }
    format!("{}\n", lines.join("\n"))
}

pub(in crate::product_cli) fn block_span(value: Option<&serde_json::Value>) -> String {
    let Some(span) = value.and_then(serde_json::Value::as_object) else {
        return String::new();
    };
    let start = span
        .get("line_start")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or_default();
    let end = span
        .get("line_end")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(start);
    if start <= 0 && end <= 0 {
        String::new()
    } else {
        format!("L{start}-L{end}")
    }
}

pub(in crate::product_cli) fn append_query_spec_lines(
    lines: &mut Vec<String>,
    query: &serde_json::Value,
    indent: &str,
) {
    lines.push(format!(
        "{indent}query {} description={}",
        block_value(value_str(query, "name")),
        block_value(value_str(query, "description"))
    ));
    let parameters = csv_values(query.get("parameters"));
    if !parameters.is_empty() {
        lines.push(format!("{indent}parameters {parameters}"));
    }
    let returns = csv_values(query.get("returns"));
    if !returns.is_empty() {
        lines.push(format!("{indent}returns {returns}"));
    }
    if let Some(statement) = query
        .get("statement")
        .or_else(|| query.get("query"))
        .and_then(serde_json::Value::as_str)
    {
        lines.push(format!("{indent}statement {}", block_value(statement)));
    }
}

pub(in crate::product_cli) fn value_array<'a>(
    payload: &'a serde_json::Value,
    key: &str,
) -> &'a [serde_json::Value] {
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(in crate::product_cli) fn value_str<'a>(payload: &'a serde_json::Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(in crate::product_cli) fn value_bool(payload: &serde_json::Value, key: &str) -> bool {
    payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(in crate::product_cli) fn csv_names(values: &[serde_json::Value]) -> String {
    values
        .iter()
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(block_value)
        .collect::<Vec<_>>()
        .join(",")
}

pub(in crate::product_cli) fn csv_values(value: Option<&serde_json::Value>) -> String {
    value
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(block_value)
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

pub(in crate::product_cli) fn block_value(value: &str) -> String {
    if value.is_empty() {
        "\"\"".to_string()
    } else if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | ':')
    }) {
        value.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    }
}
