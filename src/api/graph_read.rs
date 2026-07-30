use crate::api::catalog::support::{metadata_payload, value_array, value_str, GRAPH_SCHEMA_JSON};
use crate::db_writer::{
    connect_ladybug_database, open_ladybug_database, retry_transient_database, READ_RETRY_POLICY,
};
use lbug::{Connection, Database, SystemConfig, Value};
use serde_json::json;
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone)]
pub(crate) struct GraphSearchRequest {
    pub(crate) query: String,
    pub(crate) limit: usize,
    pub(crate) profile: String,
    pub(crate) budget: usize,
    pub(crate) context_limit: usize,
    pub(crate) max_depth: Option<usize>,
    pub(crate) detail: String,
}

pub(crate) fn count_graph_nodes(db_path: &Path) -> Result<u64, String> {
    let db = retry_transient_database(READ_RETRY_POLICY, || open_ladybug_database(db_path, true))
        .map_err(|error| error.to_string())?;
    let conn = connect_ladybug_database(&db).map_err(|error| error.to_string())?;
    let mut result = conn
        .query("MATCH (n) RETURN count(n) AS total_nodes LIMIT 1")
        .map_err(|error| format!("failed to query graph health: {error}"))?;
    let row = result
        .next()
        .ok_or_else(|| "graph health query returned no rows".to_string())?;
    row.first()
        .and_then(value_to_u64)
        .ok_or_else(|| "graph health query returned a non-numeric node count".to_string())
}

pub(crate) fn execute_graph_search(
    db_path: &Path,
    options: &GraphSearchRequest,
) -> Result<Vec<serde_json::Value>, String> {
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    crate::db_writer::preseed_ladybug_extensions(true).map_err(|error| error.to_string())?;
    conn.query("LOAD fts")
        .map_err(|error| format!("failed to load FTS extension for graph search: {error}"))?;
    let schema = metadata_payload(GRAPH_SCHEMA_JSON)?;
    let mut hits = Vec::new();
    let candidate_limit = options.limit.clamp(10, 50);
    let mut order = 0_usize;
    for index in value_array(&schema, "search_indexes") {
        let index_name = value_str(index, "name");
        for node_type in index
            .get("node_types")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            let full_index_name = format!("{index_name}_{node_type}");
            hits.extend(search_fts_index(
                &conn,
                node_type,
                &full_index_name,
                &options.query,
                candidate_limit,
                order,
            )?);
            order += 1;
        }
    }
    rank_search_hits(&mut hits, &options.query);
    hits.sort_by(|left, right| {
        right
            .rank_score
            .partial_cmp(&left.rank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.index_order.cmp(&right.index_order))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line_start.cmp(&right.line_start))
            .then_with(|| left.id.cmp(&right.id))
    });
    hits.dedup_by(|left, right| left.id == right.id);
    let mut payloads = Vec::new();
    for hit in hits.into_iter().take(options.limit) {
        let context = if options.context_limit > 0 && options.budget > 0 {
            execute_graph_context(db_path, &hit.id, &hit.node_type, options)?
        } else {
            Vec::new()
        };
        let mut payload = hit.into_json(options);
        if payload
            .get("context")
            .and_then(serde_json::Value::as_array)
            .is_some()
        {
            payload["context"] = serde_json::Value::Array(context);
        }
        payloads.push(payload);
    }
    Ok(payloads)
}

pub(crate) fn execute_graph_context(
    db_path: &Path,
    node_id: &str,
    node_type: &str,
    options: &GraphSearchRequest,
) -> Result<Vec<serde_json::Value>, String> {
    if options.context_limit == 0 || options.budget == 0 {
        return Ok(Vec::new());
    }
    let db = Database::new(db_path, SystemConfig::default().read_only(true)).map_err(|error| {
        format!(
            "failed to open graph database {}: {error}",
            db_path.display()
        )
    })?;
    let conn =
        Connection::new(&db).map_err(|error| format!("failed to connect to graph: {error}"))?;
    let schema = metadata_payload(GRAPH_SCHEMA_JSON)?;
    let profile = schema
        .get("context_profiles")
        .and_then(|profiles| profiles.get(&options.profile))
        .ok_or_else(|| format!("Unknown context profile: {}", options.profile))?;
    let relations = profile
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Context profile {} has no relations", options.profile))?;
    let depth_limit = options.max_depth.unwrap_or_else(|| {
        profile
            .get("max_depth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize
    });
    if depth_limit == 0 {
        return Ok(Vec::new());
    }
    let mut context = Vec::new();
    let mut seen = BTreeSet::new();
    let mut frontier = vec![(node_id.to_string(), node_type.to_string(), 0_usize)];
    while let Some((current_id, current_type, depth)) = frontier.first().cloned() {
        frontier.remove(0);
        if depth >= depth_limit || context.len() >= options.context_limit {
            continue;
        }
        for relation in relations.iter().filter_map(serde_json::Value::as_str) {
            for direction in ["outgoing", "incoming"] {
                let remaining = options.context_limit.saturating_sub(context.len());
                if remaining == 0 {
                    return Ok(context);
                }
                let neighbors = query_relation_neighbors(
                    &conn,
                    &schema,
                    &current_id,
                    &current_type,
                    relation,
                    direction,
                    remaining,
                )?;
                for neighbor in neighbors {
                    let neighbor_id = value_str(&neighbor, "id").to_string();
                    let neighbor_type = value_str(&neighbor, "type").to_string();
                    if neighbor_id.is_empty() || neighbor_type.is_empty() {
                        continue;
                    }
                    let dedupe_key =
                        format!("{direction}:{relation}:{neighbor_type}:{neighbor_id}");
                    if !seen.insert(dedupe_key) {
                        continue;
                    }
                    if depth + 1 < depth_limit {
                        frontier.push((neighbor_id, neighbor_type, depth + 1));
                    }
                    context.push(neighbor);
                    if context.len() >= options.context_limit {
                        return Ok(context);
                    }
                }
            }
        }
    }
    Ok(context)
}

pub(crate) fn validate_read_only_statement(statement: &str) -> Result<(), String> {
    let stripped = statement.trim().trim_end_matches(';');
    if stripped.contains(';') {
        return Err("graph_query accepts one read-only statement at a time".to_string());
    }
    for keyword in [
        "ALTER", "ATTACH", "CALL", "COPY", "CREATE", "DELETE", "DETACH", "DROP", "EXPORT",
        "IMPORT", "INSERT", "INSTALL", "LOAD", "MERGE", "REMOVE", "RENAME", "SET", "TRUNCATE",
        "UPDATE", "USE",
    ] {
        if contains_keyword(stripped, keyword) {
            return Err(format!(
                "graph_query is read-only; blocked keyword: {keyword}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn execute_read_only_query(
    db_path: &Path,
    statement: &str,
    parameters: &serde_json::Map<String, serde_json::Value>,
    limit: usize,
) -> Result<(Vec<Vec<serde_json::Value>>, bool), String> {
    let db = retry_transient_database(READ_RETRY_POLICY, || open_ladybug_database(db_path, true))
        .map_err(|error| error.to_string())?;
    let conn = connect_ladybug_database(&db).map_err(|error| error.to_string())?;
    let mut result = if parameters.is_empty() {
        conn.query(statement)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    } else {
        let named_parameters = lbug_query_parameters(parameters)?;
        let mut prepared = conn
            .prepare(statement)
            .map_err(|error| format!("failed to prepare graph query: {error}"))?;
        if !prepared.is_read_only() {
            return Err("graph-query prepared statement is not read-only".to_string());
        }
        let execute_parameters = named_parameters
            .iter()
            .map(|(name, value)| (name.as_str(), value.clone()))
            .collect();
        conn.execute(&mut prepared, execute_parameters)
            .map_err(|error| format!("failed to execute graph query: {error}"))?
    };
    let mut rows = Vec::new();
    let mut truncated = false;
    for row in result.by_ref().take(limit + 1) {
        if rows.len() == limit {
            truncated = true;
            break;
        }
        rows.push(row.into_iter().map(json_safe_value).collect());
    }
    Ok((rows, truncated))
}

pub(crate) fn cypher_single_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn cypher_identifier(value: &str) -> String {
    value.replace('`', "``")
}

fn span_json(line_start: Option<i64>, line_end: Option<i64>) -> serde_json::Value {
    let mut span = serde_json::Map::new();
    if let Some(line_start) = line_start {
        span.insert("line_start".to_string(), json!(line_start));
    }
    if let Some(line_end) = line_end {
        span.insert("line_end".to_string(), json!(line_end));
    }
    serde_json::Value::Object(span)
}

fn value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Int64(value)) => value.to_string(),
        Some(Value::UInt64(value)) => value.to_string(),
        Some(Value::Int32(value)) => value.to_string(),
        Some(Value::UInt32(value)) => value.to_string(),
        Some(Value::Null(_)) | None => String::new(),
        Some(value) => value.to_string(),
    }
}

fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int64(value)) => Some(*value),
        Some(Value::Int32(value)) => Some(i64::from(*value)),
        Some(Value::Int16(value)) => Some(i64::from(*value)),
        Some(Value::Int8(value)) => Some(i64::from(*value)),
        Some(Value::UInt64(value)) => i64::try_from(*value).ok(),
        Some(Value::UInt32(value)) => Some(i64::from(*value)),
        Some(Value::UInt16(value)) => Some(i64::from(*value)),
        Some(Value::UInt8(value)) => Some(i64::from(*value)),
        _ => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int64(value) if *value >= 0 => Some(*value as u64),
        Value::Int32(value) if *value >= 0 => Some(*value as u64),
        Value::Int16(value) if *value >= 0 => Some(*value as u64),
        Value::Int8(value) if *value >= 0 => Some(*value as u64),
        Value::UInt64(value) => Some(*value),
        Value::UInt32(value) => Some(u64::from(*value)),
        Value::UInt16(value) => Some(u64::from(*value)),
        Value::UInt8(value) => Some(u64::from(*value)),
        _ => None,
    }
}

fn value_to_f64(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Double(value)) => *value,
        Some(Value::Float(value)) => f64::from(*value),
        Some(Value::Int64(value)) => *value as f64,
        Some(Value::UInt64(value)) => *value as f64,
        Some(Value::Int32(value)) => f64::from(*value),
        Some(Value::UInt32(value)) => f64::from(*value),
        _ => 0.0,
    }
}

fn contains_keyword(statement: &str, keyword: &str) -> bool {
    let uppercase = statement.to_ascii_uppercase();
    let mut search_start = 0;
    while let Some(index) = uppercase[search_start..].find(keyword) {
        let absolute_index = search_start + index;
        let before = uppercase[..absolute_index]
            .chars()
            .next_back()
            .map(is_keyword_char)
            .unwrap_or(false);
        let after = uppercase[absolute_index + keyword.len()..]
            .chars()
            .next()
            .map(is_keyword_char)
            .unwrap_or(false);
        if !before && !after {
            return true;
        }
        search_start = absolute_index + keyword.len();
    }
    false
}

fn is_keyword_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn lbug_query_parameters(
    parameters: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(String, Value)>, String> {
    let mut converted = Vec::with_capacity(parameters.len());
    for (name, value) in parameters {
        if name.trim().is_empty() {
            return Err("graph_query parameter names must not be blank".to_string());
        }
        converted.push((name.clone(), json_parameter_to_lbug_value(value)?));
    }
    Ok(converted)
}

fn json_parameter_to_lbug_value(value: &serde_json::Value) -> Result<Value, String> {
    match value {
        serde_json::Value::Bool(value) => Ok(Value::Bool(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Int64(value))
            } else if let Some(value) = value.as_u64() {
                Ok(Value::UInt64(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Double(value))
            } else {
                Err("graph_query numeric parameter is not representable".to_string())
            }
        }
        serde_json::Value::String(value) => Ok(Value::String(value.clone())),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Ok(Value::Json(value.clone()))
        }
    }
}

fn json_safe_value(value: Value) -> serde_json::Value {
    match value {
        Value::Null(_) => serde_json::Value::Null,
        Value::Bool(value) => json!(value),
        Value::Int64(value) => json!(value),
        Value::Int32(value) => json!(value),
        Value::Int16(value) => json!(value),
        Value::Int8(value) => json!(value),
        Value::UInt64(value) => json!(value),
        Value::UInt32(value) => json!(value),
        Value::UInt16(value) => json!(value),
        Value::UInt8(value) => json!(value),
        Value::Int128(value) => json!(value.to_string()),
        Value::Double(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::Float(value) => serde_json::Number::from_f64(f64::from(value))
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| json!(value.to_string())),
        Value::String(value) => json!(value),
        Value::Json(value) => value,
        Value::List(_, values) | Value::Array(_, values) => {
            serde_json::Value::Array(values.into_iter().map(json_safe_value).collect())
        }
        Value::Struct(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, json_safe_value(value)))
                .collect(),
        ),
        other => json!(other.to_string()),
    }
}

fn query_relation_neighbors(
    conn: &Connection,
    schema: &serde_json::Value,
    node_id: &str,
    node_type: &str,
    relation: &str,
    direction: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let Some(relation_type) = relation_type(schema, relation) else {
        return Ok(Vec::new());
    };
    let source_types = relation_type
        .get("source_types")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let target_types = relation_type
        .get("target_types")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let neighbor_types = if direction == "outgoing" {
        if !source_types.contains(&node_type) {
            return Ok(Vec::new());
        }
        target_types
    } else {
        if !target_types.contains(&node_type) {
            return Ok(Vec::new());
        }
        source_types
    };
    let mut neighbors = Vec::new();
    for neighbor_type in neighbor_types {
        if neighbors.len() >= limit {
            break;
        }
        let remaining = limit - neighbors.len();
        let statement = neighbor_statement(
            node_type,
            neighbor_type,
            relation,
            direction,
            node_id,
            remaining,
        );
        let mut result = match conn.query(&statement) {
            Ok(result) => result,
            Err(error) if is_missing_search_target_error(&error.to_string()) => continue,
            Err(error) => {
                return Err(format!(
                    "failed to query {direction} {relation} neighbors for {node_type}: {error}"
                ));
            }
        };
        for row in result.by_ref() {
            let label = value_to_string(row.get(1));
            let label = if label.is_empty() {
                value_to_string(row.get(2))
            } else {
                label
            };
            let summary = value_to_string(row.get(6));
            let mut payload = json!({
                "direction": direction,
                "relation": relation,
                "type": neighbor_type,
                "label": label.clone(),
                "path": value_to_string(row.get(3)),
                "span": span_json(value_to_i64(row.get(4)), value_to_i64(row.get(5))),
                "id": value_to_string(row.first()),
            });
            if !summary.is_empty() && summary != label {
                payload["summary"] = json!(summary);
            }
            let edge_id = value_to_string(row.get(7));
            if !edge_id.is_empty() {
                payload["evidence_path"] = json!({
                    "chain": format!("{}:{}->{}", relation, value_to_string(row.get(9)), value_to_string(row.get(10)))
                });
            }
            neighbors.push(payload);
            if neighbors.len() >= limit {
                break;
            }
        }
    }
    Ok(neighbors)
}

fn relation_type<'a>(
    schema: &'a serde_json::Value,
    relation: &str,
) -> Option<&'a serde_json::Value> {
    value_array(schema, "relation_types")
        .iter()
        .find(|value| value_str(value, "name") == relation)
}

fn neighbor_statement(
    node_type: &str,
    neighbor_type: &str,
    relation: &str,
    direction: &str,
    node_id: &str,
    limit: usize,
) -> String {
    if direction == "outgoing" {
        format!(
            "MATCH (source:`{}` {{id: '{}'}})-[:`FROM_{}`]->(edge:`{}`)-[:`TO_{}`]->(neighbor:`{}`) RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, neighbor.line_start, neighbor.line_end, neighbor.summary, edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata LIMIT {}",
            cypher_identifier(node_type),
            cypher_single_quoted(node_id),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(neighbor_type),
            limit,
        )
    } else {
        format!(
            "MATCH (neighbor:`{}`)-[:`FROM_{}`]->(edge:`{}`)-[:`TO_{}`]->(target:`{}` {{id: '{}'}}) RETURN neighbor.id, neighbor.label, neighbor.qualified_name, neighbor.path, neighbor.line_start, neighbor.line_end, neighbor.summary, edge.id, edge.kind, edge.source_id, edge.target_id, edge.confidence, edge.metadata LIMIT {}",
            cypher_identifier(neighbor_type),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(relation),
            cypher_identifier(node_type),
            cypher_single_quoted(node_id),
            limit,
        )
    }
}

fn search_fts_index(
    conn: &Connection,
    node_type: &str,
    index_name: &str,
    query: &str,
    limit: usize,
    index_order: usize,
) -> Result<Vec<SearchHitRow>, String> {
    let statement = format!(
        "CALL QUERY_FTS_INDEX('{}', '{}', '{}', TOP := {}) RETURN node.id, node.label, node.qualified_name, node.path, node.line_start, node.line_end, node.summary, score",
        cypher_single_quoted(node_type),
        cypher_single_quoted(index_name),
        cypher_single_quoted(query),
        limit
    );
    let mut result = match conn.query(&statement) {
        Ok(result) => result,
        Err(error) if is_missing_search_target_error(&error.to_string()) => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to search FTS index {index_name} for {node_type}: {error}"
            ));
        }
    };
    let mut rows = Vec::new();
    for row in result.by_ref() {
        rows.push(SearchHitRow {
            id: value_to_string(row.first()),
            node_type: node_type.to_string(),
            label: value_to_string(row.get(1)),
            qualified_name: value_to_string(row.get(2)),
            path: value_to_string(row.get(3)),
            line_start: value_to_i64(row.get(4)),
            line_end: value_to_i64(row.get(5)),
            summary: value_to_string(row.get(6)),
            score: value_to_f64(row.get(7)),
            rank_score: 0.0,
            index_order,
        });
    }
    Ok(rows)
}

fn is_missing_search_target_error(error: &str) -> bool {
    error.contains("does not exist")
        || error.contains("doesn't have an index")
        || error.contains("Index not found")
}

#[derive(Debug, Clone)]
struct SearchHitRow {
    id: String,
    node_type: String,
    label: String,
    qualified_name: String,
    path: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    summary: String,
    score: f64,
    rank_score: f64,
    index_order: usize,
}

impl SearchHitRow {
    fn into_json(self, options: &GraphSearchRequest) -> serde_json::Value {
        let span = span_json(self.line_start, self.line_end);
        if options.detail == "slim" {
            let mut payload = json!({
                "id": self.id,
                "type": self.node_type,
                "label": self.label,
                "rank_score": self.rank_score,
            });
            if !self.path.is_empty() {
                payload["path"] = json!(self.path);
            }
            if !span.as_object().is_none_or(serde_json::Map::is_empty) {
                payload["span"] = span;
            }
            if !self.summary.is_empty() && self.summary != self.label {
                payload["summary"] = json!(self.summary);
            }
            return payload;
        }
        json!({
            "id": self.id,
            "type": self.node_type,
            "label": self.label,
            "qualified_name": self.qualified_name,
            "path": self.path,
            "span": span,
            "score": self.score,
            "rank_score": self.rank_score,
            "score_components": {
                "fts": self.score,
                "lexical": lexical_score(&options.query, &self),
                "entity": entity_priority_score(&self.node_type),
            },
            "summary": self.summary,
            "context": [],
        })
    }
}

fn rank_search_hits(hits: &mut [SearchHitRow], query: &str) {
    let max_score = hits.iter().map(|hit| hit.score).fold(0.0, f64::max);
    for hit in hits {
        let fts_score = if max_score > 0.0 {
            hit.score / max_score
        } else {
            0.0
        };
        let lexical = lexical_score(query, hit);
        hit.rank_score = round6(
            (0.25 * fts_score) + (0.25 * lexical) + (0.50 * entity_priority_score(&hit.node_type)),
        );
    }
}

fn lexical_score(query: &str, hit: &SearchHitRow) -> f64 {
    let normalized_query = query.to_ascii_lowercase();
    if normalized_query.is_empty() {
        return 0.0;
    }
    let label = hit.label.to_ascii_lowercase();
    let qualified_name = hit.qualified_name.to_ascii_lowercase();
    let path = hit.path.to_ascii_lowercase();
    if label == normalized_query || qualified_name == normalized_query {
        1.0
    } else if label.contains(&normalized_query) || qualified_name.contains(&normalized_query) {
        0.8
    } else if path.contains(&normalized_query) {
        0.5
    } else {
        0.0
    }
}

fn entity_priority_score(node_type: &str) -> f64 {
    match node_type {
        "Class" | "Function" | "Method" | "Module" | "Variable" | "Parameter" | "Field"
        | "Enum" | "Interface" | "Trait" | "Struct" => 1.0,
        "File" | "DocumentationChunk" | "DocumentationSource" => 0.8,
        "CallExpression" | "Reference" | "ImportDeclaration" | "Assignment" => 0.6,
        "Symbol" => 0.25,
        "Dependency" => 0.2,
        "SyntaxCapture" => 0.1,
        _ => 0.5,
    }
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
