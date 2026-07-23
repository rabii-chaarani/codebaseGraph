use crate::api::{
    contracts::{ApiError, OperationRequest, OperationResponse},
    core::{ApiCore, OperationDescriptor},
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
