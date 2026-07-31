use std::path::{Path, PathBuf};

use codebase_graph::api::{install_mcp_server, McpClientInstallOptions, McpServerDescriptor};

use crate::{
    authoring::{
        AuthoringConfig, AuthoringService, ConformanceAuthoringValidator, CreateBundleRequest,
        NoopRefreshNotifier, RepositoryRoot,
    },
    projection::ProjectionStore,
};

const DEFAULT_BUNDLE_ID: &str = "knowledge";
const DEFAULT_BUNDLE_PATH: &str = "knowledge";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    Initialized {
        state_root: PathBuf,
        bundle_root: PathBuf,
    },
    AlreadyInitialized {
        state_root: PathBuf,
        bundle_root: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpInstallRequest {
    pub client: String,
    pub scope: String,
    pub name: Option<String>,
    pub client_config_path: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    pub dry_run: bool,
}

pub fn install_repository(repository_root: &Path) -> Result<InstallOutcome, String> {
    let repository_root = repository_root
        .canonicalize()
        .map_err(|_| "the repository root could not be located".to_string())?;
    if !repository_root.is_dir() {
        return Err("the repository root is not a directory".to_string());
    }

    let bundle_root = repository_root.join(DEFAULT_BUNDLE_PATH);
    let bundle_already_exists = bundle_root.exists();
    if bundle_already_exists {
        if !bundle_root.is_dir() || !bundle_root.join("index.md").is_file() {
            return Err(
                "the default knowledge bundle already exists but is not a usable OKF bundle"
                    .to_string(),
            );
        }
    } else {
        let authoring = AuthoringService::new(
            AuthoringConfig {
                repositories: vec![RepositoryRoot {
                    id: "repository".to_string(),
                    root_path: repository_root.clone(),
                }],
                bundles: Vec::new(),
            },
            ConformanceAuthoringValidator,
            NoopRefreshNotifier,
        )
        .map_err(|_| "the repository could not be prepared for wiki authoring".to_string())?;
        authoring
            .create_bundle(CreateBundleRequest {
                bundle_id: DEFAULT_BUNDLE_ID.to_string(),
                repository_id: "repository".to_string(),
                bundle_path: DEFAULT_BUNDLE_PATH.to_string(),
                okf_version: "0.1".to_string(),
                title: Some("Repository Knowledge".to_string()),
                body_markdown: Some(
                    "Add OKF concept pages here to document this repository.\n".to_string(),
                ),
            })
            .map_err(|error| error.to_string())?;
    }

    let store = ProjectionStore::new(repository_root);
    let state_root = store.state_root();
    let already_initialized = state_root.is_dir();
    store
        .initialize()
        .map_err(|_| "the .kwiki state directory could not be initialized".to_string())?;

    Ok(if already_initialized && bundle_already_exists {
        InstallOutcome::AlreadyInitialized {
            state_root,
            bundle_root,
        }
    } else {
        InstallOutcome::Initialized {
            state_root,
            bundle_root,
        }
    })
}

pub fn install_mcp_client(request: McpInstallRequest) -> Result<serde_json::Value, String> {
    let repository_root = request
        .repo_root
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .map_err(|_| "the repository root could not be located".to_string())?;
    if !repository_root.is_dir() {
        return Err("the repository root is not a directory".to_string());
    }

    let bundle_root = repository_root.join(DEFAULT_BUNDLE_PATH);
    if !bundle_root.is_dir() || !bundle_root.join("index.md").is_file() {
        return Err(
            "the repository does not contain knowledge/index.md; run k-wiki install first"
                .to_string(),
        );
    }
    let bundle_root = bundle_root
        .canonicalize()
        .map_err(|_| "the repository knowledge bundle could not be located".to_string())?;
    if !bundle_root.starts_with(&repository_root) {
        return Err(
            "the repository knowledge bundle must remain within the repository root".to_string(),
        );
    }

    let repository_name = repository_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(install_safe_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "repository".to_string());
    let descriptor = McpServerDescriptor {
        name: request
            .name
            .unwrap_or_else(|| format!("k_wiki_{repository_name}")),
        command: std::env::var("K_WIKI_SERVER_COMMAND").unwrap_or_else(|_| "k-wiki".to_string()),
        args: vec!["mcp".to_string(), bundle_root.to_string_lossy().to_string()],
        repo_root: repository_root,
        timeout: 60,
        setup_config_path: None,
        tool_policy: Some("knowledge_wiki".to_string()),
        manual_http_metadata: None,
    };
    install_mcp_server(
        &descriptor,
        &McpClientInstallOptions {
            client: request.client,
            scope: request.scope,
            client_config_path: request.client_config_path,
            dry_run: request.dry_run,
        },
    )
}

fn install_safe_name(value: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    normalized.trim_matches(['.', '_', '-']).to_string()
}

#[cfg(test)]
mod tests {
    use super::{install_repository, InstallOutcome};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn install_repository_initializes_and_reuses_local_wiki_state() {
        let root = unique_temp_dir("install");

        let outcome = install_repository(&root).expect("initialize repository state");
        assert!(matches!(outcome, InstallOutcome::Initialized { .. }));
        for directory in [
            ".kwiki/staging",
            ".kwiki/generations",
            ".kwiki/cache",
            ".kwiki/site",
        ] {
            assert!(root.join(directory).is_dir(), "missing {directory}");
        }
        let source =
            fs::read_to_string(root.join("knowledge/index.md")).expect("read starter bundle index");
        assert!(source.contains("okf_version:"));
        assert!(source.contains("Repository Knowledge"));

        let repeat = install_repository(&root).expect("reuse repository state");
        assert!(matches!(repeat, InstallOutcome::AlreadyInitialized { .. }));

        fs::remove_dir_all(root).expect("remove temp root");
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("k_wiki_{prefix}_{}_{}", std::process::id(), unique));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }
}
