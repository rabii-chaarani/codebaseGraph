use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoSelector {
    pub repo_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    pub manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRef {
    pub id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", content = "request")]
pub enum OperationRequest {
    Health(HealthRequest),
    Search(SearchRequest),
    Context(ContextRequest),
    Query(QueryRequest),
    Materialize(MaterializationRequest),
    Plan(MaterializationRequest),
    Catalog {
        kind: String,
        group: Option<String>,
        output_format: OutputFormat,
    },
    Setup(RepositoryLifecycleRequest),
    Reinstall(RepositoryLifecycleRequest),
    Uninstall(RepositoryLifecycleRequest),
    InstallMcp(McpInstallRequest),
    Refresh(RefreshRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationInvocation {
    pub repo: RepoSelector,
    pub arguments: serde_json::Value,
    pub output_format: OutputFormat,
}

impl OperationRequest {
    pub fn operation_name(&self) -> &str {
        match self {
            Self::Health(_) => "health",
            Self::Search(_) => "search",
            Self::Context(_) => "context",
            Self::Query(_) => "query",
            Self::Materialize(_) => "materialize",
            Self::Plan(_) => "plan",
            Self::Catalog { kind, .. } => kind,
            Self::Setup(_) => "setup",
            Self::Reinstall(_) => "reinstall",
            Self::Uninstall(_) => "uninstall",
            Self::InstallMcp(_) => "mcp-install",
            Self::Refresh(_) => "refresh",
        }
    }

    pub fn repo_selector(&self) -> Option<&RepoSelector> {
        match self {
            Self::Health(request) => Some(&request.repo),
            Self::Search(request) => Some(&request.repo),
            Self::Context(request) => Some(&request.repo),
            Self::Query(request) => Some(&request.repo),
            Self::Materialize(request) => Some(&request.repo),
            Self::Plan(request) => Some(&request.repo),
            Self::Catalog { .. } => None,
            Self::Setup(request) => Some(&request.repo),
            Self::Reinstall(request) => Some(&request.repo),
            Self::Uninstall(request) => Some(&request.repo),
            Self::InstallMcp(request) => Some(&request.repo),
            Self::Refresh(request) => Some(&request.repo),
        }
    }

    pub fn output_format(&self) -> OutputFormat {
        match self {
            Self::Catalog { output_format, .. }
            | Self::Health(HealthRequest { output_format, .. })
            | Self::Search(SearchRequest { output_format, .. })
            | Self::Context(ContextRequest { output_format, .. })
            | Self::Query(QueryRequest { output_format, .. })
            | Self::Materialize(MaterializationRequest { output_format, .. })
            | Self::Plan(MaterializationRequest { output_format, .. })
            | Self::Setup(RepositoryLifecycleRequest { output_format, .. })
            | Self::Reinstall(RepositoryLifecycleRequest { output_format, .. })
            | Self::Uninstall(RepositoryLifecycleRequest { output_format, .. })
            | Self::InstallMcp(McpInstallRequest { output_format, .. })
            | Self::Refresh(RefreshRequest { output_format, .. }) => *output_format,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    #[default]
    Typed,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthRequest {
    pub repo: RepoSelector,
    pub refresh_status: Option<serde_json::Value>,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub repo: RepoSelector,
    pub query: String,
    #[serde(default = "default_graph_layer")]
    pub layer: String,
    pub profile: String,
    pub limit: usize,
    pub budget: usize,
    pub context_limit: usize,
    pub max_depth: Option<usize>,
    pub detail: String,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequest {
    pub repo: RepoSelector,
    pub query: Option<String>,
    #[serde(default = "default_graph_layer")]
    pub layer: String,
    pub profile: String,
    pub limit: usize,
    pub budget: usize,
    pub context_limit: usize,
    pub max_depth: Option<usize>,
    pub detail: String,
    pub node_id: Option<String>,
    pub node_type: Option<String>,
    pub output_format: OutputFormat,
}

fn default_graph_layer() -> String {
    "semantic".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub repo: RepoSelector,
    pub statement: String,
    pub parameters: serde_json::Value,
    pub limit: usize,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializationRequest {
    pub repo: RepoSelector,
    pub native_request_path: Option<PathBuf>,
    pub source_root: Option<String>,
    pub mode: String,
    pub include_fts: bool,
    pub semantic_enrichment: bool,
    pub semantic_provider_mode: String,
    pub use_git: bool,
    pub git_diff: bool,
    pub git_base: Option<String>,
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub candidate_paths: Vec<String>,
    pub parallel: bool,
    pub progress: bool,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryLifecycleRequest {
    pub repo: RepoSelector,
    pub action: String,
    pub output_format: OutputFormat,
    pub dry_run: bool,
    pub mcp_client: Option<String>,
    pub mcp_config_path: Option<PathBuf>,
    pub instructions_target: Option<String>,
    pub skip_mcp_config: bool,
    pub mode: String,
    pub include_fts: bool,
    pub semantic_enrichment: bool,
    pub semantic_provider_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInstallRequest {
    pub repo: RepoSelector,
    pub client: String,
    pub scope: String,
    pub name: Option<String>,
    pub client_config_path: Option<PathBuf>,
    pub dry_run: bool,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub repo: RepoSelector,
    pub paths: Vec<String>,
    pub mode: String,
    pub include_fts: bool,
    pub semantic_enrichment: bool,
    pub semantic_provider_mode: String,
    pub parallel: bool,
    pub progress: bool,
    pub output_format: OutputFormat,
}

#[derive(Clone, Copy, Debug)]
pub struct RefreshLoopConfig {
    pub poll_interval: Duration,
    pub debounce: Duration,
    pub max_wait: Duration,
    pub max_iterations: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshBackend {
    Auto,
    Native,
    Poll,
}

#[derive(Clone, Copy, Debug)]
pub struct RefreshWatchConfig {
    pub backend: RefreshBackend,
    pub loop_config: RefreshLoopConfig,
    pub once: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshWatchSummary {
    pub rebuilt: usize,
    pub deleted: usize,
    pub skipped: bool,
    pub database_written: bool,
}

pub trait RefreshWatchObserver {
    fn on_success(
        &mut self,
        backend: Option<&str>,
        summary: &RefreshWatchSummary,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String>;

    fn on_error(
        &mut self,
        backend: &str,
        error: &str,
        retrying: bool,
        event_count: usize,
        changed_paths: usize,
    ) -> Result<(), String>;

    fn on_fallback(&mut self, backend: &str, reason: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub operation: String,
    pub output_format: OutputFormat,
    pub payload: serde_json::Value,
    pub diagnostics: Vec<String>,
}

impl OperationResponse {
    pub fn from_payload(
        operation: &str,
        output_format: OutputFormat,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            operation: operation.to_string(),
            output_format,
            payload,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            details: None,
            retryable: false,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn repository() -> RepoSelector {
        RepoSelector {
            repo_root: Some(PathBuf::from("/tmp/repository")),
            config_path: Some(PathBuf::from("/tmp/config.json")),
            db_path: Some(PathBuf::from("/tmp/graph.ldb")),
            manifest_path: Some(PathBuf::from("/tmp/manifest.json")),
        }
    }

    fn materialization() -> MaterializationRequest {
        MaterializationRequest {
            repo: repository(),
            native_request_path: None,
            source_root: Some("/tmp/repository".to_string()),
            mode: "changed".to_string(),
            include_fts: true,
            semantic_enrichment: true,
            semantic_provider_mode: "local_only".to_string(),
            use_git: true,
            git_diff: false,
            git_base: None,
            include_patterns: vec!["src/**".to_string()],
            exclude_patterns: vec!["target/**".to_string()],
            candidate_paths: vec!["src/lib.rs".to_string()],
            parallel: true,
            progress: false,
            output_format: OutputFormat::Typed,
        }
    }

    fn lifecycle(action: &str) -> RepositoryLifecycleRequest {
        RepositoryLifecycleRequest {
            repo: repository(),
            action: action.to_string(),
            output_format: OutputFormat::Typed,
            dry_run: false,
            mcp_client: Some("none".to_string()),
            mcp_config_path: Some(PathBuf::from("/tmp/mcp.json")),
            instructions_target: None,
            skip_mcp_config: true,
            mode: "full".to_string(),
            include_fts: true,
            semantic_enrichment: false,
            semantic_provider_mode: "local_only".to_string(),
        }
    }

    #[test]
    fn public_operation_contracts_round_trip_without_transport_types() {
        let requests = vec![
            OperationRequest::Health(HealthRequest {
                repo: repository(),
                refresh_status: Some(json!({"running": true})),
                output_format: OutputFormat::Block,
            }),
            OperationRequest::Search(SearchRequest {
                repo: repository(),
                query: "execute operation".to_string(),
                layer: "semantic".to_string(),
                profile: "brief".to_string(),
                limit: 3,
                budget: 600,
                context_limit: 2,
                max_depth: Some(2),
                detail: "slim".to_string(),
                output_format: OutputFormat::Typed,
            }),
            OperationRequest::Context(ContextRequest {
                repo: repository(),
                query: None,
                layer: "hybrid".to_string(),
                profile: "dependencies".to_string(),
                limit: 3,
                budget: 600,
                context_limit: 2,
                max_depth: None,
                detail: "standard".to_string(),
                node_id: Some("node-1".to_string()),
                node_type: Some("Function".to_string()),
                output_format: OutputFormat::Block,
            }),
            OperationRequest::Query(QueryRequest {
                repo: repository(),
                statement: "MATCH (n) RETURN n LIMIT 1".to_string(),
                parameters: json!({}),
                limit: 1,
                output_format: OutputFormat::Typed,
            }),
            OperationRequest::Materialize(materialization()),
            OperationRequest::Plan(materialization()),
            OperationRequest::Catalog {
                kind: "architecture-queries".to_string(),
                group: Some("dependencies".to_string()),
                output_format: OutputFormat::Block,
            },
            OperationRequest::Setup(lifecycle("setup")),
            OperationRequest::Reinstall(lifecycle("reinstall")),
            OperationRequest::Uninstall(lifecycle("uninstall")),
            OperationRequest::InstallMcp(McpInstallRequest {
                repo: repository(),
                client: "generic".to_string(),
                scope: "local".to_string(),
                name: Some("codebase_graph".to_string()),
                client_config_path: Some(PathBuf::from("/tmp/mcp.json")),
                dry_run: true,
                output_format: OutputFormat::Typed,
            }),
            OperationRequest::Refresh(RefreshRequest {
                repo: repository(),
                paths: vec!["src/lib.rs".to_string()],
                mode: "changed".to_string(),
                include_fts: true,
                semantic_enrichment: true,
                semantic_provider_mode: "local_only".to_string(),
                parallel: true,
                progress: false,
                output_format: OutputFormat::Typed,
            }),
        ];

        for request in requests {
            let encoded = serde_json::to_value(&request).expect("request should serialize");
            let decoded: OperationRequest =
                serde_json::from_value(encoded.clone()).expect("request should deserialize");
            assert_eq!(
                serde_json::to_value(decoded).expect("decoded request should serialize"),
                encoded
            );
        }

        let search: SearchRequest = serde_json::from_value(json!({
            "repo": {},
            "query": "needle",
            "profile": "brief",
            "limit": 3,
            "budget": 600,
            "context_limit": 2,
            "max_depth": null,
            "detail": "standard",
            "output_format": "Typed",
        }))
        .expect("older search requests without a layer should deserialize");
        assert_eq!(search.layer, "semantic");

        let node = NodeRef {
            id: "node-1".to_string(),
            kind: "Function".to_string(),
        };
        let encoded_node = serde_json::to_value(&node).unwrap();
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<NodeRef>(encoded_node.clone()).unwrap())
                .unwrap(),
            encoded_node
        );

        let response = OperationResponse::from_payload("search", OutputFormat::Typed, json!({}));
        let encoded_response = serde_json::to_value(&response).unwrap();
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<OperationResponse>(encoded_response.clone()).unwrap()
            )
            .unwrap(),
            encoded_response
        );

        let error = ApiError::new("temporary_failure", "try again")
            .with_details(json!({"attempt": 1}))
            .retryable(true);
        let encoded_error = serde_json::to_value(&error).unwrap();
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<ApiError>(encoded_error.clone()).unwrap()
            )
            .unwrap(),
            encoded_error
        );

        let source = include_str!("contracts.rs");
        assert!(!source.contains(&["crate", "::cli"].concat()));
        assert!(!source.contains(&["mcp", "::"].concat()));

        let invocation = OperationInvocation {
            repo: repository(),
            arguments: json!({"query": "needle"}),
            output_format: OutputFormat::Block,
        };
        let encoded = serde_json::to_value(&invocation).expect("invocation should serialize");
        let decoded: OperationInvocation =
            serde_json::from_value(encoded.clone()).expect("invocation should deserialize");
        assert_eq!(
            serde_json::to_value(decoded).expect("decoded invocation should serialize"),
            encoded
        );
    }
}
