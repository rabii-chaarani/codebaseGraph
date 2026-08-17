use crate::api::{
    contracts::{
        ApiError, MaterializationRequest, OperationInvocation, OperationRequest, OperationResponse,
        RefreshWatchConfig, RefreshWatchObserver, RepoSelector,
    },
    core::{ApiCore, OperationDescriptor},
    refresh::{run_refresh_watch, start_refresh_service, RefreshServiceConfig},
};

pub trait OperationExecutor {
    fn execute(&self, request: &OperationRequest) -> Result<OperationResponse, ApiError>;
}

impl OperationExecutor for ApiCore {
    fn execute(&self, request: &OperationRequest) -> Result<OperationResponse, ApiError> {
        ApiCore::execute(self, request)
    }
}

#[derive(Debug, Clone)]
pub struct CodebaseGraphApi<C = ApiCore> {
    core: C,
}

impl CodebaseGraphApi<ApiCore> {
    pub fn new() -> Self {
        Self {
            core: ApiCore::new(),
        }
    }

    pub fn operation_descriptors(&self) -> Vec<OperationDescriptor> {
        self.core.operations()
    }

    pub fn resolve_mcp_operation(&self, tool_name: &str) -> Option<OperationDescriptor> {
        self.core.resolve_mcp_operation(tool_name)
    }

    pub(crate) fn with_auto_refresh(selector: RepoSelector, config: RefreshServiceConfig) -> Self {
        Self {
            core: ApiCore::with_refresh_state(Some(start_refresh_service(selector, config))),
        }
    }

    pub(crate) fn watch_repository(
        &self,
        request: &MaterializationRequest,
        config: RefreshWatchConfig,
        observer: &mut impl RefreshWatchObserver,
    ) -> Result<(), ApiError> {
        let runtime = crate::api::context::resolve_runtime(&request.repo)
            .map_err(|error| ApiError::new("runtime_resolution_failed", error))?;
        runtime
            .require_graph_write()
            .map_err(|message| ApiError::new("legacy_storage_requires_reinstall", message))?;
        run_refresh_watch(request, config, observer)
            .map_err(|error| ApiError::new("refresh_watch_failed", error))
    }

    pub(crate) fn latest_mcp_protocol_version() -> &'static str {
        "2025-11-25"
    }

    pub fn execute_invocation(
        &self,
        operation_id: &str,
        invocation: &OperationInvocation,
    ) -> Result<OperationResponse, ApiError> {
        self.core.execute_invocation(operation_id, invocation)
    }
}

impl<C: OperationExecutor> CodebaseGraphApi<C> {
    pub fn execute_operation(
        &self,
        request: &OperationRequest,
    ) -> Result<OperationResponse, ApiError> {
        self.core.execute(request)
    }
}

impl Default for CodebaseGraphApi<ApiCore> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::contracts::OutputFormat;
    use std::cell::Cell;

    struct SpyCore {
        calls: Cell<usize>,
    }

    impl OperationExecutor for SpyCore {
        fn execute(&self, request: &OperationRequest) -> Result<OperationResponse, ApiError> {
            self.calls.set(self.calls.get() + 1);
            Ok(OperationResponse::from_payload(
                request.operation_name(),
                request.output_format(),
                serde_json::json!({"ok": true}),
            ))
        }
    }

    #[test]
    fn execute_operation_delegates_exactly_once() {
        let api = CodebaseGraphApi {
            core: SpyCore {
                calls: Cell::new(0),
            },
        };
        let request = OperationRequest::Catalog {
            kind: "schema".to_string(),
            group: None,
            output_format: OutputFormat::Typed,
        };

        let response = api
            .execute_operation(&request)
            .expect("spy operation should succeed");

        assert_eq!(api.core.calls.get(), 1);
        assert_eq!(response.operation, "schema");
    }
}
