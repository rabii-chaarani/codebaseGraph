use crate::api::catalog::{filter_catalog, load_catalog};
use crate::api::context::{resolve_runtime, RepoRuntime};
use crate::api::contracts::{
    ApiError, ContextRequest, HealthRequest, MaterializationRequest, OperationInvocation,
    OperationRequest, OperationResponse, OutputFormat, QueryRequest, SearchRequest,
};
use crate::api::graph_read::{
    count_graph_nodes, execute_graph_context, execute_graph_search, execute_read_only_query,
    validate_read_only_statement, GraphSearchRequest,
};
use crate::api::lifecycle::{
    install_mcp_client, refresh_repository, reinstall_repository, setup_repository,
    uninstall_repository,
};
use crate::api::materialization::{
    build_request, execute_materialization_request, plan_materialization_payload, read_request,
    MaterializeOptions,
};
use crate::api::normalization::{
    normalize_request, prepare_operation_request, validate_request, DEFAULT_CONTEXT_LIMIT,
    DEFAULT_DETAIL, DEFAULT_PROFILE, DEFAULT_QUERY_LIMIT, DEFAULT_SEARCH_BUDGET,
    DEFAULT_SEARCH_LIMIT,
};
use crate::api::presenter::present_operation_response;
use crate::api::refresh::RefreshState;
use crate::error::NativeError;
use crate::protocol::{NativeSyntaxMaterializationRequest, NativeSyntaxMaterializationResponse};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub struct OperationDescriptor {
    pub id: &'static str,
    pub summary: &'static str,
    pub request_schema: fn() -> serde_json::Value,
    pub surfaces: &'static [&'static str],
    pub supported_outputs: &'static [OutputFormat],
    pub mcp_tool_name: Option<&'static str>,
}

impl OperationDescriptor {
    pub fn required_fields(&self) -> &'static [&'static str] {
        crate::api::normalization::required_fields(self.id)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OperationRegistry {
    pub(crate) operations: BTreeMap<&'static str, RegisteredOperation>,
}

type OperationHandler =
    fn(&OperationRequest, Option<&RepoRuntime>) -> Result<OperationResponse, ApiError>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RegisteredOperation {
    descriptor: OperationDescriptor,
    handler: OperationHandler,
}

#[derive(Debug, Clone, Default)]
pub struct ApiCore {
    refresh: Option<Arc<RefreshState>>,
}

impl ApiCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_refresh_state(refresh: Option<Arc<RefreshState>>) -> Self {
        Self { refresh }
    }

    pub(crate) fn register_operations(&self) -> OperationRegistry {
        let descriptors = [
            OperationDescriptor {
                id: "health",
                summary: "Check codebase graph and manifest availability",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_health"),
            },
            OperationDescriptor {
                id: "search",
                summary: "Search for code graph entities",
                request_schema: search_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_search"),
            },
            OperationDescriptor {
                id: "context",
                summary: "Return graph context for matches or explicit nodes",
                request_schema: context_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_context"),
            },
            OperationDescriptor {
                id: "query",
                summary: "Execute read-only graph query",
                request_schema: query_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_query"),
            },
            OperationDescriptor {
                id: "materialize",
                summary: "Scan and build full graph materialization",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "plan",
                summary: "Plan materialization without database write",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "schema",
                summary: "Return graph schema catalog",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_schema"),
            },
            OperationDescriptor {
                id: "query-helpers",
                summary: "Return query helper catalog",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_query_helpers"),
            },
            OperationDescriptor {
                id: "architecture-queries",
                summary: "Return architecture query catalog",
                request_schema: architecture_queries_request_schema,
                surfaces: &["api", "cli", "mcp"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: Some("graph_architecture_queries"),
            },
            OperationDescriptor {
                id: "setup",
                summary: "Install graph state and optional MCP configuration",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "reinstall",
                summary: "Rebuild graph state after backup/restore choreography",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "uninstall",
                summary: "Remove graph state and related MCP configuration",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "mcp-install",
                summary: "Register the graph tool with a supported client",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
            OperationDescriptor {
                id: "refresh",
                summary: "Refresh changed paths via incremental materialization",
                request_schema: empty_request_schema,
                surfaces: &["api", "cli"],
                supported_outputs: &[OutputFormat::Typed, OutputFormat::Block],
                mcp_tool_name: None,
            },
        ];

        build_registry(descriptors)
    }

    pub(crate) fn resolve_operation(&self, id: &str) -> Option<RegisteredOperation> {
        self.register_operations().operations.get(id).copied()
    }

    pub(crate) fn resolve_mcp_operation(&self, tool_name: &str) -> Option<OperationDescriptor> {
        self.operations()
            .into_iter()
            .find(|operation| operation.mcp_tool_name == Some(tool_name))
    }

    pub fn execute(&self, request: &OperationRequest) -> Result<OperationResponse, ApiError> {
        let mut request = normalize_request(request);
        if let (Some(refresh), OperationRequest::Health(health)) = (&self.refresh, &mut request) {
            if health.refresh_status.is_none() {
                health.refresh_status = Some(refresh.as_json());
            }
        }
        validate_request(&request)?;
        let _refresh_read_guard = if operation_requires_consistent_graph_read(&request) {
            self.refresh
                .as_ref()
                .map(|refresh| refresh.read_guard())
                .transpose()
                .map_err(|error| ApiError::new("refresh_lock_failed", error))?
        } else {
            None
        };
        let operation = self
            .resolve_operation(request.operation_name())
            .ok_or_else(|| {
                ApiError::new(
                    "unsupported_operation",
                    format!("unsupported operation: {}", request.operation_name()),
                )
            })?;
        let runtime = request
            .repo_selector()
            .map(resolve_runtime)
            .transpose()
            .map_err(|error| ApiError::new("runtime_resolution_failed", error))?;
        let response = dispatch_operation(&request, &operation, runtime.as_ref())?;
        Ok(present_operation_response(
            response,
            request.output_format(),
        ))
    }

    pub(crate) fn execute_invocation(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
    ) -> Result<OperationResponse, ApiError> {
        let request = prepare_operation_request(operation_id, invocation)?;
        self.execute(&request)
    }

    pub(crate) fn operations(&self) -> Vec<OperationDescriptor> {
        let mut items: Vec<_> = self
            .register_operations()
            .operations
            .values()
            .map(|operation| operation.descriptor)
            .collect();
        items.sort_by_key(|descriptor| descriptor.id);
        items
    }
}

fn build_registry(descriptors: impl IntoIterator<Item = OperationDescriptor>) -> OperationRegistry {
    let mut by_id = BTreeMap::new();
    for descriptor in descriptors {
        let handler: OperationHandler = match descriptor.id {
            "health" => handle_health,
            "search" => handle_search,
            "context" => handle_context,
            "query" => handle_query,
            "materialize" => handle_materialize,
            "plan" => handle_plan,
            "schema" | "query-helpers" | "architecture-queries" => handle_catalog,
            "setup" => handle_setup,
            "reinstall" => handle_reinstall,
            "uninstall" => handle_uninstall,
            "mcp-install" => handle_mcp_install,
            "refresh" => handle_refresh,
            id => panic!("operation {id} does not have a handler registration"),
        };
        assert!(
            !descriptor.supported_outputs.is_empty(),
            "operation {} must expose at least one output format",
            descriptor.id
        );
        assert_eq!(
            descriptor.surfaces.contains(&"mcp"),
            descriptor.mcp_tool_name.is_some(),
            "operation {} has inconsistent MCP exposure",
            descriptor.id
        );
        if by_id
            .insert(
                descriptor.id,
                RegisteredOperation {
                    descriptor,
                    handler,
                },
            )
            .is_some()
        {
            panic!("duplicate operation registration: {}", descriptor.id);
        }
    }
    OperationRegistry { operations: by_id }
}

fn empty_request_schema() -> serde_json::Value {
    json!({})
}

fn search_request_schema() -> serde_json::Value {
    json!({
        "query": {"type": "string"},
        "limit": {"type": "integer", "minimum": 1, "default": DEFAULT_SEARCH_LIMIT},
        "profile": {"type": "string", "default": DEFAULT_PROFILE},
        "budget": {"type": "integer", "minimum": 0, "default": DEFAULT_SEARCH_BUDGET},
        "context_limit": {"type": "integer", "minimum": 0, "default": DEFAULT_CONTEXT_LIMIT},
        "max_depth": {"type": "integer", "minimum": 0},
        "detail": {"type": "string", "enum": ["standard", "slim"], "default": DEFAULT_DETAIL},
    })
}

fn context_request_schema() -> serde_json::Value {
    json!({
        "query": {"type": "string"},
        "node_id": {"type": "string"},
        "node_type": {"type": "string"},
        "limit": {"type": "integer", "minimum": 1, "default": DEFAULT_SEARCH_LIMIT},
        "profile": {"type": "string", "default": DEFAULT_PROFILE},
        "budget": {"type": "integer", "minimum": 0, "default": DEFAULT_SEARCH_BUDGET},
        "context_limit": {"type": "integer", "minimum": 0, "default": DEFAULT_CONTEXT_LIMIT},
        "max_depth": {"type": "integer", "minimum": 0},
        "detail": {"type": "string", "enum": ["standard", "slim"], "default": DEFAULT_DETAIL},
    })
}

fn query_request_schema() -> serde_json::Value {
    json!({
        "statement": {"type": "string"},
        "parameters": {"type": "object"},
        "query": {"type": "string"},
        "limit": {"type": "integer", "minimum": 1, "maximum": 1000, "default": DEFAULT_QUERY_LIMIT},
    })
}

fn architecture_queries_request_schema() -> serde_json::Value {
    json!({
        "group": {"type": "string"},
    })
}

fn dispatch_operation(
    request: &OperationRequest,
    operation: &RegisteredOperation,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    (operation.handler)(request, runtime)
}

fn operation_requires_consistent_graph_read(request: &OperationRequest) -> bool {
    matches!(
        request,
        OperationRequest::Health(_)
            | OperationRequest::Search(_)
            | OperationRequest::Context(_)
            | OperationRequest::Query(_)
    )
}

fn required_runtime<'a>(
    operation: &str,
    runtime: Option<&'a RepoRuntime>,
) -> Result<&'a RepoRuntime, ApiError> {
    runtime.ok_or_else(|| {
        ApiError::new(
            "runtime_resolution_failed",
            format!("{operation} operation requires a repository selector"),
        )
    })
}

fn invalid_request(expected: &str, request: &OperationRequest) -> ApiError {
    ApiError::new(
        "invalid_operation_request",
        format!(
            "operation handler expected {expected}, received {}",
            request.operation_name()
        ),
    )
}

fn handle_health(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Health(request) = request else {
        return Err(invalid_request("health", request));
    };
    execute_health(request, required_runtime("health", runtime)?)
}

fn handle_search(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Search(request) = request else {
        return Err(invalid_request("search", request));
    };
    execute_search(request, required_runtime("search", runtime)?)
}

fn handle_context(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Context(request) = request else {
        return Err(invalid_request("context", request));
    };
    execute_context(request, required_runtime("context", runtime)?)
}

fn handle_query(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Query(request) = request else {
        return Err(invalid_request("query", request));
    };
    execute_query(request, required_runtime("query", runtime)?)
}

fn handle_materialize(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Materialize(request) = request else {
        return Err(invalid_request("materialize", request));
    };
    execute_materialization(request, required_runtime("materialize", runtime)?, false)
}

fn handle_plan(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Plan(request) = request else {
        return Err(invalid_request("plan", request));
    };
    execute_materialization(request, required_runtime("plan", runtime)?, true)
}

fn handle_catalog(
    request: &OperationRequest,
    _runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Catalog { kind, group, .. } = request else {
        return Err(invalid_request("catalog", request));
    };
    let mut payload =
        load_catalog(kind).map_err(|error| ApiError::new("invalid_catalog_kind", error))?;
    filter_catalog(kind, &mut payload, group.as_deref())
        .map_err(|error| ApiError::new("invalid_catalog_group", error))?;
    Ok(OperationResponse::from_payload(
        kind,
        OutputFormat::Typed,
        payload,
    ))
}

fn handle_setup(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Setup(request) = request else {
        return Err(invalid_request("setup", request));
    };
    Ok(OperationResponse::from_payload(
        "setup",
        OutputFormat::Typed,
        setup_repository(request, required_runtime("setup", runtime)?)?,
    ))
}

fn handle_reinstall(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Reinstall(request) = request else {
        return Err(invalid_request("reinstall", request));
    };
    Ok(OperationResponse::from_payload(
        "reinstall",
        OutputFormat::Typed,
        reinstall_repository(request, required_runtime("reinstall", runtime)?)?,
    ))
}

fn handle_uninstall(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Uninstall(request) = request else {
        return Err(invalid_request("uninstall", request));
    };
    Ok(OperationResponse::from_payload(
        "uninstall",
        OutputFormat::Typed,
        uninstall_repository(request, required_runtime("uninstall", runtime)?)?,
    ))
}

fn handle_mcp_install(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::InstallMcp(request) = request else {
        return Err(invalid_request("mcp-install", request));
    };
    Ok(OperationResponse::from_payload(
        "mcp-install",
        OutputFormat::Typed,
        install_mcp_client(request, required_runtime("mcp-install", runtime)?)?,
    ))
}

fn handle_refresh(
    request: &OperationRequest,
    runtime: Option<&RepoRuntime>,
) -> Result<OperationResponse, ApiError> {
    let OperationRequest::Refresh(request) = request else {
        return Err(invalid_request("refresh", request));
    };
    Ok(OperationResponse::from_payload(
        "refresh",
        OutputFormat::Typed,
        refresh_repository(request, required_runtime("refresh", runtime)?)?,
    ))
}

fn execute_health(
    request: &HealthRequest,
    runtime: &RepoRuntime,
) -> Result<OperationResponse, ApiError> {
    let output_format = request.output_format;

    let mut graph_readable = false;
    let mut total_nodes = 0_u64;
    let mut error_message = None;
    let database_exists = runtime.db_path.exists();
    let manifest_exists = runtime.manifest_path.exists();

    if database_exists {
        match count_graph_nodes(&runtime.db_path) {
            Ok(count) => {
                graph_readable = true;
                total_nodes = count;
            }
            Err(error) => {
                error_message = Some(error);
            }
        }
    } else {
        error_message = Some(format!(
            "database file does not exist: {}",
            runtime.db_path.display()
        ));
    }

    let payload = json!({
        "ok": database_exists && graph_readable,
        "repo_root": runtime.repo_root,
        "database_path": runtime.db_path,
        "manifest_path": runtime.manifest_path,
        "database_exists": database_exists,
        "manifest_exists": manifest_exists,
        "graph_readable": graph_readable,
        "total_nodes": total_nodes,
        "error": error_message,
        "refresh": request.refresh_status,
    });
    Ok(OperationResponse::from_payload(
        "health",
        output_format,
        payload,
    ))
}

fn execute_search(
    request: &SearchRequest,
    runtime: &RepoRuntime,
) -> Result<OperationResponse, ApiError> {
    let output_format = request.output_format;
    let options = GraphSearchRequest {
        query: request.query.clone(),
        limit: request.limit,
        profile: request.profile.clone(),
        budget: request.budget,
        context_limit: request.context_limit,
        max_depth: request.max_depth,
        detail: request.detail.clone(),
    };
    let results = execute_graph_search(&runtime.db_path, &options)
        .map_err(|error| ApiError::new("search_execution_failed", error.to_string()))?;
    let payload = serde_json::json!({
        "query": request.query,
        "profile": request.profile,
        "limit": request.limit,
        "budget": request.budget,
        "results": results,
    });
    Ok(OperationResponse::from_payload(
        "search",
        output_format,
        payload,
    ))
}

fn execute_context(
    request: &ContextRequest,
    runtime: &RepoRuntime,
) -> Result<OperationResponse, ApiError> {
    let output_format = request.output_format;
    let search = GraphSearchRequest {
        query: request.query.clone().unwrap_or_default(),
        profile: request.profile.clone(),
        limit: request.limit,
        budget: request.budget,
        context_limit: request.context_limit,
        max_depth: request.max_depth,
        detail: request.detail.clone(),
    };
    let payload = if let (Some(node_id), Some(node_type)) =
        (request.node_id.as_ref(), request.node_type.as_ref())
    {
        let context = execute_graph_context(&runtime.db_path, node_id, node_type, &search)
            .map_err(|error| ApiError::new("context_execution_failed", error.to_string()))?;
        json!({
            "node_id": node_id,
            "node_type": node_type,
            "profile": request.profile,
            "context": context,
        })
    } else {
        if search.query.is_empty() {
            return Err(ApiError::new(
                "context_validation_failed",
                "query is required",
            ));
        }
        let results = execute_graph_search(&runtime.db_path, &search)
            .map_err(|error| ApiError::new("context_execution_failed", error.to_string()))?;
        json!({
            "query": request.query,
            "profile": request.profile,
            "limit": request.limit,
            "budget": request.budget,
            "results": results,
        })
    };
    Ok(OperationResponse::from_payload(
        "context",
        output_format,
        payload,
    ))
}

fn execute_query(
    request: &QueryRequest,
    runtime: &RepoRuntime,
) -> Result<OperationResponse, ApiError> {
    validate_read_only_statement(&request.statement)
        .map_err(|error| ApiError::new("query_validation_failed", error.to_string()))?;
    let output_format = request.output_format;
    let parameters = request
        .parameters
        .as_object()
        .expect("normalized query parameters must be an object")
        .clone();
    let (rows, truncated) = execute_read_only_query(
        &runtime.db_path,
        &request.statement,
        &parameters,
        request.limit,
    )
    .map_err(|error| ApiError::new("query_execution_failed", error.to_string()))?;
    let payload = serde_json::json!({
        "statement": request.statement,
        "row_count": rows.len(),
        "rows": rows,
        "truncated": truncated,
    });
    Ok(OperationResponse::from_payload(
        "query",
        output_format,
        payload,
    ))
}

fn execute_materialization(
    request: &MaterializationRequest,
    runtime: &RepoRuntime,
    dry_plan: bool,
) -> Result<OperationResponse, ApiError> {
    let output_format = request.output_format;
    let materialize_options = MaterializeOptions::from_request(request, runtime, dry_plan);

    let mut native_request = if let Some(request_path) = request.native_request_path.as_ref() {
        read_request(request_path)
            .map_err(|error| ApiError::new("materialization_request_failed", error))?
    } else {
        build_request(&materialize_options)
            .map_err(|error| ApiError::new("materialization_build_failed", error))?
    };

    let response = if dry_plan {
        native_request.atomic_rebuild = false;
        execute_plan_native(&native_request)
            .map_err(|error| ApiError::new("materialization_plan_failed", error.to_string()))?
    } else {
        execute_materialization_request(&materialize_options, native_request)
            .map(|(_, response)| response)
            .map_err(|error| ApiError::new("materialization_failed", error))?
    };

    let payload = materialization_payload(request, &response, &runtime.manifest_path, dry_plan);
    let mut operation_response = OperationResponse::from_payload(
        if dry_plan { "plan" } else { "materialize" },
        output_format,
        payload,
    );
    operation_response.diagnostics = response.diagnostics.clone();
    Ok(operation_response)
}

fn execute_plan_native(
    request: &NativeSyntaxMaterializationRequest,
) -> Result<NativeSyntaxMaterializationResponse, NativeError> {
    crate::plan_materialization(request)
}

fn materialization_payload(
    request: &MaterializationRequest,
    response: &NativeSyntaxMaterializationResponse,
    manifest_path: &std::path::Path,
    dry_plan: bool,
) -> serde_json::Value {
    if dry_plan {
        plan_materialization_payload(response, &request.mode, manifest_path)
    } else {
        serde_json::to_value(response).unwrap_or_else(|_| json!({}))
    }
}

#[cfg(test)]
mod tests {
    use crate::api::contracts::{McpInstallRequest, OperationRequest, OutputFormat, RepoSelector};
    use crate::api::core::ApiCore;
    use crate::api::{
        install_mcp_server, McpClientInstallOptions, McpExistingEntryPolicy, McpInstallMode,
        McpServerDescriptor,
    };
    use serde_json::json;
    use std::{fs, path::PathBuf};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn operation_registry_is_unique_and_sorted() {
        let core = ApiCore::new();
        let operations = core.operations();
        let ids: Vec<_> = operations.iter().map(|entry| entry.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert!(ids.iter().all(|id| core.resolve_operation(id).is_some()));
    }

    #[test]
    fn resolve_unknown_operation_is_none() {
        let core = ApiCore::new();
        assert!(core.resolve_operation("_missing_").is_none());
    }

    #[test]
    fn duplicate_operations_are_rejected_in_registration() {
        let core = ApiCore::new();
        let descriptor = core
            .operations()
            .into_iter()
            .find(|descriptor| descriptor.id == "health")
            .expect("health descriptor should exist");
        let result = std::panic::catch_unwind(|| super::build_registry([descriptor, descriptor]));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_operation_request_is_rejected() {
        let core = ApiCore::new();
        let unsupported = OperationRequest::Catalog {
            kind: "_missing_".to_string(),
            group: None,
            output_format: crate::api::contracts::OutputFormat::Typed,
        };
        let error = core
            .execute(&unsupported)
            .expect_err("unknown operation should be rejected");
        assert_eq!(error.code, "unsupported_operation");
        assert!(!error.retryable);
    }

    #[test]
    fn resolve_operation_returns_registered_descriptor_and_handler() {
        let core = ApiCore::new();
        let operation = core
            .resolve_operation("schema")
            .expect("schema should be registered");
        assert_eq!(operation.descriptor.id, "schema");
        let response = (operation.handler)(
            &OperationRequest::Catalog {
                kind: "schema".to_string(),
                group: None,
                output_format: crate::api::contracts::OutputFormat::Typed,
            },
            None,
        )
        .expect("registered schema handler should execute");
        assert_eq!(response.operation, "schema");
    }

    #[test]
    fn resolve_mcp_operation_uses_registered_operation_metadata() {
        let core = ApiCore::new();
        let operation = core
            .resolve_mcp_operation("graph_search")
            .expect("registered MCP operation should resolve");
        assert_eq!(operation.id, "search");
        assert!(core.resolve_mcp_operation("graph_missing").is_none());
    }

    #[test]
    fn refresh_read_policy_covers_repository_graph_reads() {
        let repo = RepoSelector::default();
        let typed = OutputFormat::Typed;
        assert!(super::operation_requires_consistent_graph_read(
            &OperationRequest::Health(crate::api::contracts::HealthRequest {
                repo: repo.clone(),
                refresh_status: None,
                output_format: typed,
            })
        ));
        assert!(super::operation_requires_consistent_graph_read(
            &OperationRequest::Query(crate::api::contracts::QueryRequest {
                repo,
                statement: "MATCH (n) RETURN n".to_string(),
                parameters: json!({}),
                limit: 1,
                output_format: typed,
            })
        ));
        assert!(!super::operation_requires_consistent_graph_read(
            &OperationRequest::Catalog {
                kind: "schema".to_string(),
                group: None,
                output_format: typed,
            }
        ));
    }

    #[test]
    fn mcp_install_operation_preserves_unrelated_client_configuration() {
        let root = unique_temp_dir("codebase-graph-api-mcp-install");
        let client_config = root.join("client").join("mcp.json");
        fs::create_dir_all(client_config.parent().unwrap()).unwrap();
        fs::write(
            &client_config,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "unrelated": {"command": "keep", "args": []}
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let response = ApiCore::new()
            .execute(&OperationRequest::InstallMcp(McpInstallRequest {
                repo: RepoSelector {
                    repo_root: Some(root.clone()),
                    config_path: None,
                    db_path: None,
                    manifest_path: None,
                },
                client: "generic".to_string(),
                scope: "local".to_string(),
                name: Some("codebase_graph_test".to_string()),
                client_config_path: Some(client_config.clone()),
                dry_run: false,
                output_format: OutputFormat::Typed,
            }))
            .expect("API MCP install should succeed");

        assert_eq!(response.operation, "mcp-install");
        assert_eq!(response.payload["action"], "updated");
        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
        assert_eq!(payload["mcpServers"]["unrelated"]["command"], "keep");
        assert_eq!(
            payload["mcpServers"]["codebase_graph_test"]["command"],
            "codebase-graph"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn generic_mcp_server_registration_preserves_configuration_with_short_arguments() {
        let root = unique_temp_dir("codebase-graph-generic-mcp-server");
        let client_config = root.join("client").join("mcp.json");
        fs::create_dir_all(client_config.parent().unwrap()).unwrap();
        fs::write(
            &client_config,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "unrelated": {"command": "keep", "args": []}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let knowledge = root.join("knowledge");
        let descriptor = McpServerDescriptor {
            name: "k_wiki_test".to_string(),
            command: "k-wiki".to_string(),
            args: vec!["mcp".to_string(), knowledge.to_string_lossy().to_string()],
            repo_root: root.clone(),
            timeout: 60,
            setup_config_path: None,
            tool_policy: Some("knowledge_wiki".to_string()),
            manual_http_metadata: None,
        };
        let options = McpClientInstallOptions {
            client: "generic".to_string(),
            scope: "local".to_string(),
            client_config_path: Some(client_config.clone()),
            dry_run: false,
            install_method: McpInstallMode::Auto,
            existing_entry_policy: McpExistingEntryPolicy::Replace,
            legacy_server_names: Vec::new(),
        };

        let response = install_mcp_server(&descriptor, &options).expect("register MCP server");
        assert_eq!(response["action"], "updated");
        let payload: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&client_config).unwrap()).unwrap();
        assert_eq!(payload["mcpServers"]["unrelated"]["command"], "keep");
        assert_eq!(payload["mcpServers"]["k_wiki_test"]["command"], "k-wiki");
        assert_eq!(
            payload["mcpServers"]["k_wiki_test"]["args"],
            json!(["mcp", knowledge.to_string_lossy().to_string()])
        );

        let repeat = install_mcp_server(&descriptor, &options).expect("reuse MCP registration");
        assert_eq!(repeat["action"], "unchanged");
        let _ = fs::remove_dir_all(root);
    }
}
