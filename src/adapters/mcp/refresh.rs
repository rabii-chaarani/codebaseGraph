use super::options::McpServeOptions;
use crate::api::CodebaseGraphApi;

pub(in crate::adapters) fn start_auto_refresh(options: &McpServeOptions) -> CodebaseGraphApi {
    CodebaseGraphApi::with_auto_refresh(options.repo_selector())
}
