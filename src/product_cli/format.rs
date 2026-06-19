use super::*;

pub(super) fn metadata_payload(source: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(source)
        .map_err(|error| format!("failed to parse embedded metadata: {error}"))
}

pub(super) fn write_metadata_output<W: Write>(
    stdout: &mut W,
    payload: &serde_json::Value,
    options: &MetadataOutputOptions,
    block_serializer: fn(&serde_json::Value) -> String,
) -> Result<(), String> {
    let text = if options.format == "json" {
        if options.pretty {
            serde_json::to_string_pretty(payload).map_err(|error| error.to_string())?
        } else {
            serde_json::to_string(payload).map_err(|error| error.to_string())?
        }
    } else {
        block_serializer(payload)
    };
    writeln!(stdout, "{text}").map_err(|error| error.to_string())
}

pub(super) fn filter_architecture_group(
    payload: &mut serde_json::Value,
    group: &str,
) -> Result<(), String> {
    let groups = payload
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected: Vec<serde_json::Value> = groups
        .iter()
        .filter(|value| value.get("name").and_then(serde_json::Value::as_str) == Some(group))
        .cloned()
        .collect();
    if selected.is_empty() {
        let valid = groups
            .iter()
            .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Unknown architecture query group: {group}. Valid groups: {valid}"
        ));
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert("groups".to_string(), serde_json::Value::Array(selected));
    }
    Ok(())
}

pub(super) fn serialize_schema_block(payload: &serde_json::Value) -> String {
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

pub(super) fn serialize_query_helpers_block(payload: &serde_json::Value) -> String {
    let helpers = value_array(payload, "query_helpers");
    let mut lines = vec![format!("query_helpers count={}", helpers.len())];
    for helper in helpers {
        append_query_spec_lines(&mut lines, helper, "");
    }
    format!("{}\n", lines.join("\n"))
}

pub(super) fn serialize_architecture_queries_block(payload: &serde_json::Value) -> String {
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

pub(super) fn serialize_health_block(payload: &serde_json::Value) -> String {
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

pub(super) fn serialize_search_block(payload: &serde_json::Value) -> String {
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

pub(super) fn serialize_query_block(payload: &serde_json::Value) -> String {
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

pub(super) fn query_columns(statement: &str) -> Vec<String> {
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

pub(super) fn split_return_expressions(text: &str) -> Vec<String> {
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

pub(super) fn query_column_label(expression: &str) -> Option<String> {
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

pub(super) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

pub(super) fn block_json_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => block_value(value),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".to_string()),
    }
}

pub(super) fn serialize_error_block(payload: &serde_json::Value) -> String {
    let error = payload.get("error").unwrap_or(payload);
    format!(
        "error tool={} type={} message={}\n",
        block_value(value_str(error, "tool")),
        block_value(value_str(error, "type")),
        block_value(value_str(error, "message"))
    )
}

pub(super) fn serialize_context_block(payload: &serde_json::Value) -> String {
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

pub(super) fn block_span(value: Option<&serde_json::Value>) -> String {
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

pub(super) fn append_query_spec_lines(
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

pub(super) fn value_array<'a>(
    payload: &'a serde_json::Value,
    key: &str,
) -> &'a [serde_json::Value] {
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(super) fn value_str<'a>(payload: &'a serde_json::Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

pub(super) fn value_bool(payload: &serde_json::Value, key: &str) -> bool {
    payload
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(super) fn csv_names(values: &[serde_json::Value]) -> String {
    values
        .iter()
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(block_value)
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn csv_values(value: Option<&serde_json::Value>) -> String {
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

pub(super) fn block_value(value: &str) -> String {
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

pub(super) fn top_level_help() -> &'static str {
    "codebase-graph native CLI\n\nUSAGE:\n  codebase-graph <command> [options]\n\nCOMMANDS:\n  setup                       Materialize graph state and write .codebaseGraph/config.json\n  materialize                 Materialize a graph through the Rust native engine\n  plan                        Preview files that would rebuild, delete, skip, or ignore\n  watch                       Watch for file changes and refresh after a debounce window\n  graph-health                Check whether the native graph database is readable\n  graph-schema                Return ontology schema, indexes, profiles, and helpers\n  graph-query-helpers         Return named read-only graph query helpers\n  graph-architecture-queries  Return the architecture-discovery query catalog\n  graph-search, search        Search the code graph with compact context\n  graph-context, context      Return compact graph context\n  graph-query                 Execute a restricted read-only graph query\n  mcp                         Serve codebaseGraph MCP over stdio or HTTP\n\nRun `codebase-graph <command> --help` for command options."
}

pub(super) fn mcp_help() -> &'static str {
    "codebase-graph mcp\n\nUSAGE:\n  codebase-graph mcp install [--client <client>] [--scope <scope>] [--config-path <path>] [--client-config-path <path>] [--dry-run] [--json]\n  codebase-graph mcp serve [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>]\n  codebase-graph mcp http [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--host <host>] [--port <port>] [--path <path>] [--allow-remote] [--auth-token <token>|--auth-token-env <name>]\n\nOPTIONS:\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --host <host>             HTTP bind host; defaults to 127.0.0.1\n  --port <port>             HTTP bind port; defaults to 8765\n  --path <path>             HTTP endpoint path; defaults to /mcp\n  --allow-remote            Permit non-local HTTP bind when an auth token is supplied\n  --auth-token <token>      Bearer token required for HTTP requests\n  --auth-token-env <name>   Environment variable containing the bearer token"
}

pub(super) fn mcp_install_help() -> &'static str {
    "codebase-graph mcp install\n\nUSAGE:\n  codebase-graph mcp install [--client <client>] [--scope local|user|project] [--name <name>] [--config-path <path>] [--client-config-path <path>] [--repo-root <path>] [--dry-run] [--verify] [--json]\n\nOPTIONS:\n  --client <client>             codex, claude, claude-project, lmstudio, github-copilot, hermes, openclaw, generic, copilot-studio, microsoft-copilot, or all\n  --scope <scope>               local, user, or project; defaults to local\n  --name <name>                 MCP server name; defaults to codebase_graph_<repo>\n  --config-path <path>          Path to .codebaseGraph/config.json\n  --client-config-path <path>   Override the target MCP client config path\n  --repo-root <path>            Repository root used to find .codebaseGraph/config.json\n  --dry-run                     Show install action without writing files or invoking CLIs\n  --verify                      Accepted for compatibility\n  --json                        Emit JSON output"
}

pub(super) fn materialize_help() -> &'static str {
    "codebase-graph materialize\n\nUSAGE:\n  codebase-graph materialize [--source-root <path>|--repo-root <path>] [--db <path>] [--manifest <path>] [--mode full|changed] [--json]\n  codebase-graph materialize --native-request <path> [--manifest <path>] [--json]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to scan\n  --repo-root <path>        Alias for --source-root\n  --db <path>               Ladybug database path; defaults under .codebaseGraph\n  --manifest <path>         Manifest path; defaults under .codebaseGraph\n  --mode <mode>             full or changed; defaults to changed\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Materialize files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --parallel                Parse independent files concurrently\n  --single-thread           Force single-thread parsing\n  --progress                Include progress events in JSON output\n  --no-fts                  Skip FTS extension loading and index creation\n  --no-semantic-enrichment  Skip semantic enrichment\n  --semantic-provider-mode  local_only only; provider-backed modes are not supported by Rust-only production\n  --native-request <path>   JSON NativeSyntaxMaterializationRequest payload\n  --json                    Emit JSON output"
}

pub(super) fn plan_help() -> &'static str {
    "codebase-graph plan\n\nUSAGE:\n  codebase-graph plan [--source-root <path>|--repo-root <path>] [--manifest <path>] [--mode full|changed] [--json]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to scan\n  --repo-root <path>        Alias for --source-root\n  --manifest <path>         Manifest path; defaults under .codebaseGraph\n  --mode <mode>             full or changed; defaults to changed\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Plan files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --native-request <path>   JSON NativeSyntaxMaterializationRequest payload\n  --json                    Emit JSON output"
}

pub(super) fn watch_help() -> &'static str {
    "codebase-graph watch\n\nUSAGE:\n  codebase-graph watch [--source-root <path>|--repo-root <path>] [--mode full|changed] [--watch-backend auto|native|poll] [--poll-ms <n>] [--debounce-ms <n>]\n\nOPTIONS:\n  --source-root <path>      Repository or source root to watch recursively\n  --repo-root <path>        Alias for --source-root\n  --mode <mode>             full or changed; defaults to changed\n  --watch-backend <backend> auto, native, or poll; defaults to auto\n  --poll-ms <n>             Poll interval for poll backend or auto fallback; defaults to 500\n  --debounce-ms <n>         Quiet-window debounce interval in milliseconds; defaults to 250\n  --max-iterations <n>      Stop after n refreshes, useful for tests\n  --once                    Run one refresh immediately and exit\n  --no-git                  Disable Git file discovery and scan the filesystem\n  --git-diff                Refresh files from git diff plus untracked files\n  --git-base <rev>          Revision used by --git-diff; defaults to HEAD\n  --include <glob>          Include only paths matching the glob; repeatable\n  --exclude <glob>          Exclude paths matching the glob; repeatable\n  --parallel                Parse independent files concurrently\n  --progress                Include progress events in JSON output"
}

pub(super) fn setup_help() -> &'static str {
    "codebase-graph setup\n\nUSAGE:\n  codebase-graph setup [--repo-root <path>] [--mode full|changed] [--mcp-client <client>] [--mcp-config-path <path>] [--skip-mcp-config] [--dry-run] [--instructions-target auto|agents|claude|skip] [--json]\n\nOPTIONS:\n  --repo-root <path>          Repository root to initialize\n  --mode <mode>               full or changed; defaults to changed\n  --mcp-client <client>       codex, claude, claude-project, lmstudio, github-copilot, hermes, openclaw, generic, copilot-studio, microsoft-copilot, or none\n  --mcp-config-path <path>    Override MCP client config path\n  --skip-mcp-config           Do not write MCP client config\n  --dry-run                   Report setup changes without writing repo or client state\n  --instructions-target <t>   auto, agents, claude, or skip\n  --no-fts                    Skip FTS extension loading and index creation\n  --no-semantic-enrichment    Skip semantic enrichment\n  --semantic-provider-mode    local_only only; provider-backed modes are not supported by Rust-only production\n  --json                      Emit JSON output"
}

pub(super) fn graph_health_help() -> &'static str {
    "codebase-graph graph-health\n\nUSAGE:\n  codebase-graph graph-health [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--json]\n\nOPTIONS:\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --json                    Emit JSON output"
}

pub(super) fn graph_schema_help() -> &'static str {
    "codebase-graph graph-schema\n\nUSAGE:\n  codebase-graph graph-schema [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

pub(super) fn graph_query_helpers_help() -> &'static str {
    "codebase-graph graph-query-helpers\n\nUSAGE:\n  codebase-graph graph-query-helpers [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

pub(super) fn graph_architecture_queries_help() -> &'static str {
    "codebase-graph graph-architecture-queries\n\nUSAGE:\n  codebase-graph graph-architecture-queries [--group <name>] [--format json|block] [--json] [--pretty]\n\nOPTIONS:\n  --group <name>            Optional architecture query group to return\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output\n  --pretty                  Pretty-print JSON output"
}

pub(super) fn graph_search_help() -> &'static str {
    "codebase-graph graph-search\n\nUSAGE:\n  codebase-graph graph-search <query> [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <n>] [--profile <name>] [--detail standard|slim] [--format json|block] [--json]\n\nOPTIONS:\n  <query>                   Search query\n  --limit <n>               Maximum search hits; defaults to 3\n  --profile <name>          Context profile label; defaults to brief\n  --budget <n>              Context budget retained in output payload; defaults to 600\n  --context-limit <n>       Context item limit retained for compatibility\n  --detail <level>          standard or slim; defaults to standard\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output"
}

pub(super) fn graph_context_help() -> &'static str {
    "codebase-graph graph-context\n\nUSAGE:\n  codebase-graph graph-context [query] [--node-id <id> --node-type <type>] [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <n>] [--context-limit <n>] [--profile <name>] [--detail standard|slim] [--format json|block] [--json]\n\nOPTIONS:\n  [query]                   Search query used when explicit node lookup is not supplied\n  --node-id <id>            Explicit graph node id\n  --node-type <type>        Explicit graph node type\n  --limit <n>               Maximum search hits in query mode; defaults to 3\n  --context-limit <n>       Maximum explicit context rows; defaults to 3\n  --profile <name>          Context profile label; defaults to brief\n  --budget <n>              Context budget retained in output payload; defaults to 600\n  --detail <level>          standard or slim; defaults to standard\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --format <format>         block or json; defaults to block\n  --json                    Emit compact JSON output"
}

pub(super) fn metadata_help(command_name: &str) -> &'static str {
    match command_name {
        "graph-schema" => graph_schema_help(),
        "graph-query-helpers" => graph_query_helpers_help(),
        "graph-architecture-queries" => graph_architecture_queries_help(),
        "graph-search" => graph_search_help(),
        "graph-context" => graph_context_help(),
        _ => "codebase-graph metadata command",
    }
}

pub(super) fn graph_query_help() -> &'static str {
    "codebase-graph graph-query\n\nUSAGE:\n  codebase-graph graph-query <statement> [--repo-root <path>] [--config <path>] [--db <path>] [--manifest <path>] [--limit <rows>] [--parameters <json>] [--json]\n\nOPTIONS:\n  <statement>               Restricted read-only Cypher statement\n  --parameters <json>       JSON object with named query parameters\n  --limit <rows>            Maximum rows to return; defaults to 100 and caps at 1000\n  --repo-root <path>        Repository root to inspect\n  --config <path>           Setup config path; defaults to .codebaseGraph/config.json\n  --db <path>               Ladybug database path override\n  --manifest <path>         Manifest path override\n  --json                    Emit JSON output"
}
pub(super) fn schema_statements_from_copy_statements(
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

pub(super) fn fts_index_statements(node_tables: &[String]) -> Vec<String> {
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

pub(super) fn copy_tables(copy_statements: &[String]) -> BTreeSet<String> {
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

pub(super) fn relation_names(tables: &BTreeSet<String>) -> BTreeSet<String> {
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

pub(super) fn node_table_sql(table: &str, fields: Vec<(&'static str, &'static str)>) -> String {
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

pub(super) fn relation_table_sql(
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

pub(super) fn node_fields(table: &str) -> Vec<(&'static str, &'static str)> {
    let mut fields = common_node_fields();
    if table == "File" {
        fields.push(("content_hash", "STRING"));
    }
    fields
}

pub(super) fn common_node_fields() -> Vec<(&'static str, &'static str)> {
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

pub(super) fn edge_fields() -> Vec<(&'static str, &'static str)> {
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
