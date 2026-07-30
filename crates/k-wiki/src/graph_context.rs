//! Optional, bounded composition with the public source-graph API.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::api::{GraphContextItem, GraphContextStatus, GraphContextSummary};

const MAX_RESULTS: usize = 10;
const MAX_SUMMARY_CHARS: usize = 240;

pub trait GraphClient {
    fn search(&self, repository_root: &Path, query: &str, limit: usize) -> Result<Value, String>;

    fn context(
        &self,
        repository_root: &Path,
        node_id: &str,
        node_type: &str,
        limit: usize,
    ) -> Result<Value, String>;
}

#[derive(Clone, Debug)]
pub struct GraphContextAdapter<C> {
    client: Option<C>,
    result_limit: usize,
}

impl<C> GraphContextAdapter<C>
where
    C: GraphClient,
{
    pub fn disabled() -> Self {
        Self {
            client: None,
            result_limit: MAX_RESULTS,
        }
    }

    pub fn new(client: C) -> Self {
        Self {
            client: Some(client),
            result_limit: MAX_RESULTS,
        }
    }

    pub fn with_result_limit(mut self, limit: usize) -> Self {
        self.result_limit = limit.clamp(1, MAX_RESULTS);
        self
    }

    pub fn search(&self, repository_root: &Path, query: &str) -> GraphContextSummary {
        let Some(client) = &self.client else {
            return summary(GraphContextStatus::Disabled, Vec::new());
        };
        match client.search(repository_root, query, self.result_limit) {
            Ok(payload) => summary(
                GraphContextStatus::Available,
                translate_results(&payload, self.result_limit),
            ),
            Err(_) => summary(GraphContextStatus::Degraded, Vec::new()),
        }
    }

    pub fn context(
        &self,
        repository_root: &Path,
        node_id: &str,
        node_type: &str,
    ) -> GraphContextSummary {
        let Some(client) = &self.client else {
            return summary(GraphContextStatus::Disabled, Vec::new());
        };
        match client.context(repository_root, node_id, node_type, self.result_limit) {
            Ok(payload) => summary(
                GraphContextStatus::Available,
                translate_context(&payload, self.result_limit),
            ),
            Err(_) => summary(GraphContextStatus::Degraded, Vec::new()),
        }
    }
}

fn summary(status: GraphContextStatus, items: Vec<GraphContextItem>) -> GraphContextSummary {
    GraphContextSummary { status, items }
}

fn translate_results(payload: &Value, limit: usize) -> Vec<GraphContextItem> {
    payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(translate_item)
        .collect()
}

fn translate_context(payload: &Value, limit: usize) -> Vec<GraphContextItem> {
    if payload.get("results").is_some() {
        return translate_results(payload, limit);
    }
    payload
        .get("context")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(translate_item)
        .collect()
}

fn translate_item(value: &Value) -> Option<GraphContextItem> {
    let id = value.get("id")?.as_str()?.to_string();
    let kind = value
        .get("type")
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let label = value.get("label")?.as_str()?.to_string();
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| is_safe_relative_path(path))
        .map(ToOwned::to_owned);
    let summary = value
        .get("summary")
        .and_then(Value::as_str)
        .map(|text| truncate(text, MAX_SUMMARY_CHARS));
    Some(GraphContextItem {
        id,
        kind,
        label,
        path,
        summary,
    })
}

fn is_safe_relative_path(path: &str) -> bool {
    let path = PathBuf::from(path);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn truncate(text: &str, max_chars: usize) -> String {
    let mut output = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        output.push('…');
    }
    output
}

#[cfg(feature = "graph-context")]
#[derive(Debug, Default)]
pub struct PublicGraphClient {
    api: codebase_graph::api::CodebaseGraphApi,
}

#[cfg(feature = "graph-context")]
impl PublicGraphClient {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(feature = "graph-context")]
impl GraphClient for PublicGraphClient {
    fn search(&self, repository_root: &Path, query: &str, limit: usize) -> Result<Value, String> {
        use codebase_graph::api::{OperationRequest, OutputFormat, SearchRequest};
        let response = self
            .api
            .execute_operation(&OperationRequest::Search(SearchRequest {
                repo: selector(repository_root),
                query: query.to_string(),
                profile: "brief".to_string(),
                limit: limit.min(MAX_RESULTS),
                budget: 600,
                context_limit: 0,
                max_depth: Some(1),
                detail: "slim".to_string(),
                output_format: OutputFormat::Typed,
            }))
            .map_err(|_| "graph context unavailable".to_string())?;
        Ok(response.payload)
    }

    fn context(
        &self,
        repository_root: &Path,
        node_id: &str,
        node_type: &str,
        limit: usize,
    ) -> Result<Value, String> {
        use codebase_graph::api::{ContextRequest, OperationRequest, OutputFormat};
        let response = self
            .api
            .execute_operation(&OperationRequest::Context(ContextRequest {
                repo: selector(repository_root),
                query: None,
                profile: "definitions".to_string(),
                limit: limit.min(MAX_RESULTS),
                budget: 600,
                context_limit: 2,
                max_depth: Some(1),
                detail: "slim".to_string(),
                node_id: Some(node_id.to_string()),
                node_type: Some(node_type.to_string()),
                output_format: OutputFormat::Typed,
            }))
            .map_err(|_| "graph context unavailable".to_string())?;
        Ok(response.payload)
    }
}

#[cfg(feature = "graph-context")]
fn selector(repository_root: &Path) -> codebase_graph::api::RepoSelector {
    codebase_graph::api::RepoSelector {
        repo_root: Some(repository_root.to_path_buf()),
        config_path: None,
        db_path: None,
        manifest_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Clone)]
    struct FakeClient(Result<Value, String>);

    impl GraphClient for FakeClient {
        fn search(&self, _root: &Path, _query: &str, _limit: usize) -> Result<Value, String> {
            self.0.clone()
        }

        fn context(
            &self,
            _root: &Path,
            _node_id: &str,
            _node_type: &str,
            _limit: usize,
        ) -> Result<Value, String> {
            self.0.clone()
        }
    }

    #[test]
    fn public_results_are_bounded_and_path_safe() {
        let adapter = GraphContextAdapter::new(FakeClient(Ok(json!({
            "results": [{
                "id": "n1",
                "type": "Function",
                "label": "handler",
                "path": "/secret/repo/src.rs",
                "summary": "x".repeat(300),
                "raw": "not exposed"
            }]
        }))));
        let result = adapter.search(Path::new("."), "handler");

        assert_eq!(result.status, GraphContextStatus::Available);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].path, None);
        assert!(result.items[0].summary.as_ref().unwrap().ends_with('…'));
    }

    #[test]
    fn graph_failures_degrade_without_propagating_raw_errors() {
        let adapter = GraphContextAdapter::new(FakeClient(Err(
            "/private/repository/database failed".to_string(),
        )));
        let result = adapter.search(Path::new("."), "handler");

        assert_eq!(result.status, GraphContextStatus::Degraded);
        assert!(result.items.is_empty());
    }
}
