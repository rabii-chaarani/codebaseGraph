use crate::api::contracts::{
    ApiError, ContextRequest, HealthRequest, MaterializationRequest, McpInstallRequest,
    OperationInvocation, OperationRequest, QueryRequest, RefreshRequest,
    RepositoryLifecycleRequest, SearchRequest,
};
use crate::api::lifecycle::{expand_path, supported_mcp_clients};
use crate::api::materialization::MaterializeOptions;
use serde_json::json;

pub(crate) const DEFAULT_PROFILE: &str = "brief";
pub(crate) const DEFAULT_DETAIL: &str = "standard";
pub(crate) const DEFAULT_SEARCH_LIMIT: usize = 3;
pub(crate) const DEFAULT_SEARCH_BUDGET: usize = 600;
pub(crate) const DEFAULT_CONTEXT_LIMIT: usize = 3;
pub(crate) const DEFAULT_QUERY_LIMIT: usize = 100;
const DEFAULT_MATERIALIZATION_MODE: &str = "changed";
const DEFAULT_SEMANTIC_PROVIDER_MODE: &str = "local_only";

pub(crate) fn prepare_operation_request(
    operation_id: &str,
    invocation: &OperationInvocation,
) -> Result<OperationRequest, ApiError> {
    let arguments = &invocation.arguments;
    let request = match operation_id {
        "health" => OperationRequest::Health(HealthRequest {
            repo: invocation.repo.clone(),
            refresh_status: None,
            output_format: invocation.output_format,
        }),
        "search" => OperationRequest::Search(SearchRequest {
            repo: invocation.repo.clone(),
            query: argument_string(arguments, "query"),
            profile: argument_string(arguments, "profile"),
            limit: argument_usize(arguments, "limit").unwrap_or(DEFAULT_SEARCH_LIMIT),
            budget: argument_usize(arguments, "budget").unwrap_or(DEFAULT_SEARCH_BUDGET),
            context_limit: argument_usize(arguments, "context_limit")
                .unwrap_or(DEFAULT_CONTEXT_LIMIT),
            max_depth: argument_usize(arguments, "max_depth"),
            detail: argument_string(arguments, "detail"),
            output_format: invocation.output_format,
        }),
        "context" => OperationRequest::Context(ContextRequest {
            repo: invocation.repo.clone(),
            query: argument_optional_string(arguments, "query"),
            profile: argument_string(arguments, "profile"),
            limit: argument_usize(arguments, "limit").unwrap_or(DEFAULT_SEARCH_LIMIT),
            budget: argument_usize(arguments, "budget").unwrap_or(DEFAULT_SEARCH_BUDGET),
            context_limit: argument_usize(arguments, "context_limit")
                .unwrap_or(DEFAULT_CONTEXT_LIMIT),
            max_depth: argument_usize(arguments, "max_depth"),
            detail: argument_string(arguments, "detail"),
            node_id: argument_optional_string(arguments, "node_id"),
            node_type: argument_optional_string(arguments, "node_type"),
            output_format: invocation.output_format,
        }),
        "query" => OperationRequest::Query(QueryRequest {
            repo: invocation.repo.clone(),
            statement: argument_string(arguments, "statement"),
            parameters: arguments
                .get("parameters")
                .cloned()
                .unwrap_or_else(|| json!({})),
            limit: argument_usize(arguments, "limit").unwrap_or(DEFAULT_QUERY_LIMIT),
            output_format: invocation.output_format,
        }),
        "schema" | "query-helpers" => OperationRequest::Catalog {
            kind: operation_id.to_string(),
            group: None,
            output_format: invocation.output_format,
        },
        "architecture-queries" => OperationRequest::Catalog {
            kind: operation_id.to_string(),
            group: argument_optional_string(arguments, "group"),
            output_format: invocation.output_format,
        },
        "syntax" => OperationRequest::Catalog {
            kind: operation_id.to_string(),
            group: argument_optional_string(arguments, "language"),
            output_format: invocation.output_format,
        },
        _ => {
            return Err(ApiError::new(
                "unsupported_operation",
                format!("unsupported registered operation input: {operation_id}"),
            ))
        }
    };
    Ok(normalize_request(&request))
}

pub(crate) fn normalize_request(request: &OperationRequest) -> OperationRequest {
    let mut normalized = request.clone();
    match &mut normalized {
        OperationRequest::Search(request) => {
            request.query = request.query.trim().to_string();
            default_string(&mut request.profile, DEFAULT_PROFILE);
            default_string(&mut request.detail, DEFAULT_DETAIL);
        }
        OperationRequest::Context(request) => {
            default_string(&mut request.profile, DEFAULT_PROFILE);
            default_string(&mut request.detail, DEFAULT_DETAIL);
            request.query = request
                .query
                .take()
                .map(|query| query.trim().to_string())
                .filter(|query| !query.is_empty());
            request.node_id = normalized_optional_string(request.node_id.take());
            request.node_type = normalized_optional_string(request.node_type.take());
        }
        OperationRequest::Query(request) => {
            request.statement = request.statement.trim().to_string();
        }
        OperationRequest::Materialize(request) | OperationRequest::Plan(request) => {
            normalize_materialization(request);
        }
        OperationRequest::Setup(request)
        | OperationRequest::Reinstall(request)
        | OperationRequest::Uninstall(request) => {
            normalize_lifecycle(request);
        }
        OperationRequest::InstallMcp(request) => normalize_mcp_install(request),
        OperationRequest::Refresh(request) => normalize_refresh(request),
        OperationRequest::Catalog { kind, group, .. } => {
            *kind = kind.trim().to_string();
            *group = normalized_optional_string(group.take());
        }
        OperationRequest::Health(_) => {}
    }
    normalized
}

pub(crate) fn normalize_materialize_options(options: &mut MaterializeOptions) {
    default_string(&mut options.mode, DEFAULT_MATERIALIZATION_MODE);
    default_string(
        &mut options.semantic_provider_mode,
        DEFAULT_SEMANTIC_PROVIDER_MODE,
    );
    normalize_paths(&mut options.candidate_paths);
}

pub(crate) fn validate_request(request: &OperationRequest) -> Result<(), ApiError> {
    match request {
        OperationRequest::Search(request) => {
            validate_search_fields(&request.query, &request.detail, request.limit)
        }
        OperationRequest::Context(request) => {
            if request.node_id.is_some() != request.node_type.is_some() {
                return Err(ApiError::new(
                    "invalid_node_reference",
                    "context operation requires both node_id and node_type",
                ));
            }
            if request.query.is_none() && request.node_id.is_none() {
                return Err(ApiError::new(
                    "missing_query",
                    "context operation requires a query or explicit node reference",
                ));
            }
            validate_detail_and_limit(&request.detail, request.limit)
        }
        OperationRequest::Query(request) => {
            if request.statement.is_empty() {
                return Err(ApiError::new(
                    "missing_query",
                    "graph_query requires a non-empty statement",
                ));
            }
            if request.limit == 0 || request.limit > 1000 {
                return Err(ApiError::new(
                    "invalid_limit",
                    "graph_query limit must be between 1 and 1000",
                ));
            }
            if !request.parameters.is_object() {
                return Err(ApiError::new(
                    "query_invalid_parameters",
                    "query parameters must be an object",
                ));
            }
            Ok(())
        }
        OperationRequest::Materialize(request) | OperationRequest::Plan(request) => {
            validate_materialization_fields(
                &request.mode,
                &request.semantic_provider_mode,
                "materialization",
            )
        }
        OperationRequest::Refresh(request) => validate_materialization_fields(
            &request.mode,
            &request.semantic_provider_mode,
            "refresh",
        ),
        OperationRequest::Setup(request) => validate_lifecycle(request, "setup"),
        OperationRequest::Reinstall(request) => validate_lifecycle(request, "reinstall"),
        OperationRequest::Uninstall(request) => validate_lifecycle(request, "uninstall"),
        OperationRequest::InstallMcp(request) => validate_mcp_install(request),
        OperationRequest::Catalog { kind, group, .. } => {
            if kind.is_empty() {
                Err(ApiError::new(
                    "invalid_catalog_kind",
                    "catalog kind must not be empty",
                ))
            } else if kind == "syntax" && group.is_none() {
                Err(ApiError::new(
                    "invalid_catalog_group",
                    "syntax catalog requires a language",
                ))
            } else {
                Ok(())
            }
        }
        OperationRequest::Health(_) => Ok(()),
    }
}

pub(crate) fn required_fields(operation_id: &str) -> &'static [&'static str] {
    match operation_id {
        "search" => &["query"],
        "query" => &["statement"],
        "syntax" => &["language"],
        _ => &[],
    }
}

fn normalize_materialization(request: &mut MaterializationRequest) {
    default_string(&mut request.mode, DEFAULT_MATERIALIZATION_MODE);
    default_string(
        &mut request.semantic_provider_mode,
        DEFAULT_SEMANTIC_PROVIDER_MODE,
    );
    request.source_root = normalized_optional_string(request.source_root.take());
    if request.repo.repo_root.is_none() {
        request.repo.repo_root = request.source_root.as_deref().map(std::path::PathBuf::from);
    }
    request.git_base = normalized_optional_string(request.git_base.take());
    normalize_paths(&mut request.candidate_paths);
}

fn normalize_refresh(request: &mut RefreshRequest) {
    default_string(&mut request.mode, DEFAULT_MATERIALIZATION_MODE);
    default_string(
        &mut request.semantic_provider_mode,
        DEFAULT_SEMANTIC_PROVIDER_MODE,
    );
    normalize_paths(&mut request.paths);
}

fn normalize_lifecycle(request: &mut RepositoryLifecycleRequest) {
    request.action = request.action.trim().to_string();
    default_string(&mut request.mode, DEFAULT_MATERIALIZATION_MODE);
    default_string(
        &mut request.semantic_provider_mode,
        DEFAULT_SEMANTIC_PROVIDER_MODE,
    );
    request.mcp_client = normalized_optional_string(request.mcp_client.take());
    request.instructions_target = normalized_optional_string(request.instructions_target.take());
}

fn normalize_mcp_install(request: &mut McpInstallRequest) {
    request.client = request.client.trim().to_ascii_lowercase();
    request.scope = request.scope.trim().to_ascii_lowercase();
    request.name = normalized_optional_string(request.name.take());
    request.client_config_path = request
        .client_config_path
        .take()
        .map(|path| expand_path(&path.to_string_lossy()));
    request.repo.repo_root = request
        .repo
        .repo_root
        .take()
        .map(|path| expand_path(&path.to_string_lossy()));
    request.repo.config_path = request
        .repo
        .config_path
        .take()
        .map(|path| expand_path(&path.to_string_lossy()));
}

fn validate_search_fields(query: &str, detail: &str, limit: usize) -> Result<(), ApiError> {
    if query.trim().is_empty() {
        return Err(ApiError::new(
            "missing_query",
            "Search query must not be empty",
        ));
    }
    validate_detail_and_limit(detail, limit)
}

fn validate_detail_and_limit(detail: &str, limit: usize) -> Result<(), ApiError> {
    if detail != "standard" && detail != "slim" {
        return Err(ApiError::new(
            "invalid_detail",
            "detail must be standard or slim",
        ));
    }
    if limit == 0 {
        return Err(ApiError::new(
            "invalid_limit",
            "search limit must be greater than zero",
        ));
    }
    Ok(())
}

fn validate_materialization_fields(
    mode: &str,
    semantic_provider_mode: &str,
    operation: &str,
) -> Result<(), ApiError> {
    if mode != "full" && mode != "changed" {
        return Err(ApiError::new(
            "invalid_materialization_mode",
            format!("{operation} mode must be full or changed"),
        ));
    }
    if semantic_provider_mode != "local_only" {
        return Err(ApiError::new(
            "invalid_semantic_provider_mode",
            format!("{operation} semantic provider mode must be local_only"),
        ));
    }
    Ok(())
}

fn validate_lifecycle(
    request: &RepositoryLifecycleRequest,
    expected_action: &str,
) -> Result<(), ApiError> {
    if request.action != expected_action {
        return Err(ApiError::new(
            "invalid_lifecycle_action",
            format!(
                "lifecycle request action {} does not match {expected_action}",
                request.action
            ),
        ));
    }
    validate_materialization_fields(
        &request.mode,
        &request.semantic_provider_mode,
        expected_action,
    )?;
    if let Some(client) = request.mcp_client.as_deref() {
        let special_allowed = match expected_action {
            "uninstall" => client == "all",
            _ => client == "none",
        };
        if !special_allowed && !supported_mcp_clients().contains(&client) {
            return Err(ApiError::new(
                "invalid_mcp_client",
                format!("unsupported MCP client: {client}"),
            ));
        }
    }
    Ok(())
}

fn validate_mcp_install(request: &McpInstallRequest) -> Result<(), ApiError> {
    if request.client != "all" && !supported_mcp_clients().contains(&request.client.as_str()) {
        return Err(ApiError::new(
            "invalid_mcp_client",
            format!("unsupported MCP client: {}", request.client),
        ));
    }
    if !matches!(request.scope.as_str(), "local" | "user" | "project") {
        return Err(ApiError::new(
            "invalid_mcp_scope",
            "MCP install scope must be local, user, or project",
        ));
    }
    if request.client == "all" && request.client_config_path.is_some() {
        return Err(ApiError::new(
            "invalid_mcp_config_path",
            "client_config_path requires one selected MCP client",
        ));
    }
    Ok(())
}

fn default_string(value: &mut String, default: &str) {
    let normalized = value.trim();
    *value = if normalized.is_empty() {
        default.to_string()
    } else {
        normalized.to_string()
    };
}

fn normalized_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn argument_string(arguments: &serde_json::Value, key: &str) -> String {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn argument_optional_string(arguments: &serde_json::Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn argument_usize(arguments: &serde_json::Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn normalize_paths(paths: &mut Vec<String>) {
    for path in paths.iter_mut() {
        *path = path.trim().trim_start_matches("./").replace('\\', "/");
    }
    paths.retain(|path| !path.is_empty());
    paths.sort();
    paths.dedup();
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_request, prepare_operation_request, required_fields, validate_request,
        DEFAULT_CONTEXT_LIMIT, DEFAULT_DETAIL, DEFAULT_PROFILE, DEFAULT_QUERY_LIMIT,
        DEFAULT_SEARCH_BUDGET, DEFAULT_SEARCH_LIMIT,
    };
    use crate::api::contracts::{
        ContextRequest, MaterializationRequest, McpInstallRequest, OperationInvocation,
        OperationRequest, OutputFormat, RepoSelector, SearchRequest,
    };
    use serde_json::json;

    #[test]
    fn normalize_request_applies_canonical_defaults() {
        let normalized = normalize_request(&OperationRequest::Search(SearchRequest {
            repo: RepoSelector::default(),
            query: "needle".to_string(),
            profile: " ".to_string(),
            limit: 3,
            budget: 600,
            context_limit: 3,
            max_depth: None,
            detail: String::new(),
            output_format: OutputFormat::Typed,
        }));

        let OperationRequest::Search(search) = normalized else {
            panic!("search request should remain a search request");
        };
        assert_eq!(search.profile, "brief");
        assert_eq!(search.detail, "standard");

        let normalized =
            normalize_request(&OperationRequest::Materialize(MaterializationRequest {
                repo: RepoSelector::default(),
                native_request_path: None,
                source_root: Some(" ./repository ".to_string()),
                mode: String::new(),
                include_fts: true,
                semantic_enrichment: true,
                semantic_provider_mode: String::new(),
                use_git: false,
                git_diff: false,
                git_base: None,
                include_patterns: Vec::new(),
                exclude_patterns: Vec::new(),
                candidate_paths: Vec::new(),
                parallel: true,
                progress: false,
                output_format: OutputFormat::Typed,
            }));
        let OperationRequest::Materialize(materialization) = normalized else {
            panic!("materialization request should remain a materialization request");
        };
        assert_eq!(materialization.mode, "changed");
        assert_eq!(materialization.semantic_provider_mode, "local_only");
        assert_eq!(
            materialization.repo.repo_root,
            Some(std::path::PathBuf::from("./repository"))
        );
    }

    #[test]
    fn prepare_operation_request_applies_canonical_registered_input_defaults() {
        let invocation = OperationInvocation {
            repo: RepoSelector::default(),
            arguments: json!({"query": "needle"}),
            output_format: OutputFormat::Typed,
        };

        let request = prepare_operation_request("search", &invocation)
            .expect("registered search input should prepare");
        let OperationRequest::Search(search) = request else {
            panic!("registered search input should produce a search request");
        };
        assert_eq!(search.profile, DEFAULT_PROFILE);
        assert_eq!(search.detail, DEFAULT_DETAIL);
        assert_eq!(search.limit, DEFAULT_SEARCH_LIMIT);
        assert_eq!(search.budget, DEFAULT_SEARCH_BUDGET);
        assert_eq!(search.context_limit, DEFAULT_CONTEXT_LIMIT);

        let query = prepare_operation_request(
            "query",
            &OperationInvocation {
                arguments: json!({"statement": "MATCH (n) RETURN n"}),
                ..invocation.clone()
            },
        )
        .expect("registered query input should prepare");
        let OperationRequest::Query(query) = query else {
            panic!("registered query input should produce a query request");
        };
        assert_eq!(query.limit, DEFAULT_QUERY_LIMIT);
        assert_eq!(query.parameters, json!({}));

        let syntax = prepare_operation_request(
            "syntax",
            &OperationInvocation {
                arguments: json!({"language": " rust "}),
                ..invocation
            },
        )
        .expect("registered syntax input should prepare");
        let OperationRequest::Catalog { kind, group, .. } = syntax else {
            panic!("registered syntax input should produce a catalog request");
        };
        assert_eq!(kind, "syntax");
        assert_eq!(group.as_deref(), Some("rust"));
    }

    #[test]
    fn validate_request_rejects_invalid_operation_rules_before_execution() {
        let error = validate_request(&OperationRequest::Context(ContextRequest {
            repo: RepoSelector::default(),
            query: None,
            profile: "brief".to_string(),
            limit: 3,
            budget: 600,
            context_limit: 3,
            max_depth: None,
            detail: "standard".to_string(),
            node_id: Some("Function:1".to_string()),
            node_type: None,
            output_format: OutputFormat::Typed,
        }))
        .expect_err("partial node reference should be rejected");

        assert_eq!(error.code, "invalid_node_reference");

        let syntax_error = validate_request(&OperationRequest::Catalog {
            kind: "syntax".to_string(),
            group: None,
            output_format: OutputFormat::Typed,
        })
        .expect_err("syntax catalog without a language should be rejected");
        assert_eq!(syntax_error.code, "invalid_catalog_group");
    }

    #[test]
    fn validate_request_rejects_unsupported_mcp_clients_and_scopes() {
        let request = |client: &str, scope: &str| {
            OperationRequest::InstallMcp(McpInstallRequest {
                repo: RepoSelector::default(),
                client: client.to_string(),
                scope: scope.to_string(),
                name: None,
                client_config_path: None,
                dry_run: true,
                output_format: OutputFormat::Typed,
            })
        };

        let client_error = validate_request(&request("unknown-client", "local"))
            .expect_err("unsupported clients should be rejected by the API");
        assert_eq!(client_error.code, "invalid_mcp_client");

        let scope_error = validate_request(&request("generic", "unknown-scope"))
            .expect_err("unsupported scopes should be rejected by the API");
        assert_eq!(scope_error.code, "invalid_mcp_scope");
    }

    #[test]
    fn operation_schema_requirements_share_normalization_policy() {
        assert_eq!(required_fields("search"), ["query"]);
        assert_eq!(required_fields("query"), ["statement"]);
        assert_eq!(required_fields("syntax"), ["language"]);
        assert!(required_fields("context").is_empty());
    }
}
