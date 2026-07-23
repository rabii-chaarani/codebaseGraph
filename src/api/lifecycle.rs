use crate::api::context::RepoRuntime;
use crate::api::contracts::{ApiError, RefreshRequest, RepositoryLifecycleRequest};
use crate::api::materialization::execute_candidate_materialization;
use crate::cli::materialization::MaterializeOptions;
use crate::cli::{reinstall, setup, uninstall};

pub(crate) fn is_retryable_refresh_failure(error: &str) -> bool {
    crate::db_writer::is_transient_database_error(error)
}

pub(crate) fn setup_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "setup")?;
    setup::execute_setup_operation(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("setup_failed", error))
}

pub(crate) fn reinstall_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "reinstall")?;
    reinstall::execute_reinstall_operation(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("reinstall_failed", error))
}

pub(crate) fn uninstall_repository(
    request: &RepositoryLifecycleRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    validate_lifecycle_action(request, "uninstall")?;
    uninstall::execute_uninstall_operation(request, &runtime.repo_root)
        .map_err(|error| ApiError::new("uninstall_failed", error))
}

fn validate_lifecycle_action(
    request: &RepositoryLifecycleRequest,
    expected: &str,
) -> Result<(), ApiError> {
    if request.action == expected {
        Ok(())
    } else {
        Err(ApiError::new(
            "invalid_lifecycle_action",
            format!(
                "lifecycle request action {} does not match {expected}",
                request.action
            ),
        ))
    }
}

pub(crate) fn refresh_repository(
    request: &RefreshRequest,
    runtime: &RepoRuntime,
) -> Result<serde_json::Value, ApiError> {
    let repo_root = request
        .repo
        .repo_root
        .clone()
        .unwrap_or_else(|| runtime.repo_root.clone());
    let options = MaterializeOptions {
        source_root: Some(repo_root),
        db: Some(
            request
                .repo
                .db_path
                .clone()
                .unwrap_or_else(|| runtime.db_path.clone()),
        ),
        manifest: Some(
            request
                .repo
                .manifest_path
                .clone()
                .unwrap_or_else(|| runtime.manifest_path.clone()),
        ),
        use_git: false,
        mode: request.mode.clone(),
        include_fts: request.include_fts,
        semantic_enrichment: request.semantic_enrichment,
        semantic_provider_mode: request.semantic_provider_mode.clone(),
        git_diff: false,
        git_base: None,
        include_patterns: Vec::new(),
        exclude_patterns: Vec::new(),
        candidate_paths: request.paths.clone(),
        parallel: request.parallel,
        progress: request.progress,
        plan_only: false,
        ..MaterializeOptions::default()
    };

    let (_request, response) =
        execute_candidate_materialization(&options, options.candidate_paths.clone()).map_err(
            |error| {
                let retryable = is_retryable_refresh_failure(&error);
                ApiError::new("refresh_failed", error).retryable(retryable)
            },
        )?;
    Ok(serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({})))
}
