use super::{WikiApiError, WikiOperationRequest, WikiOperationResponse};

pub trait WikiOperationExecutor {
    fn execute(
        &self,
        request: &WikiOperationRequest,
    ) -> Result<WikiOperationResponse, WikiApiError>;
}

impl<F> WikiOperationExecutor for F
where
    F: Fn(&WikiOperationRequest) -> Result<WikiOperationResponse, WikiApiError>,
{
    fn execute(
        &self,
        request: &WikiOperationRequest,
    ) -> Result<WikiOperationResponse, WikiApiError> {
        self(request)
    }
}

#[derive(Clone, Debug)]
pub struct OkfWikiApi<E> {
    executor: E,
}

impl<E> OkfWikiApi<E>
where
    E: WikiOperationExecutor,
{
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn execute_operation(
        &self,
        request: &WikiOperationRequest,
    ) -> Result<WikiOperationResponse, WikiApiError> {
        self.executor.execute(request)
    }
}

impl<E> WikiOperationExecutor for OkfWikiApi<E>
where
    E: WikiOperationExecutor,
{
    fn execute(
        &self,
        request: &WikiOperationRequest,
    ) -> Result<WikiOperationResponse, WikiApiError> {
        self.execute_operation(request)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::api::{HealthRequest, HealthResponse};

    use super::*;

    #[test]
    fn facade_dispatches_each_request_exactly_once() {
        let calls = Rc::new(Cell::new(0));
        let observed = Rc::clone(&calls);
        let api = OkfWikiApi::new(move |_request: &WikiOperationRequest| {
            observed.set(observed.get() + 1);
            Ok(WikiOperationResponse::Health(HealthResponse {
                ok: true,
                schema_version: 1,
                projection_available: false,
            }))
        });

        let response = api
            .execute_operation(&WikiOperationRequest::Health(HealthRequest::default()))
            .expect("execute health");

        assert!(matches!(response, WikiOperationResponse::Health(_)));
        assert_eq!(calls.get(), 1);
    }
}
