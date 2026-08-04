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
const INSTRUCTION_START: &str = "<!-- k-wiki:start -->";
const INSTRUCTION_END: &str = "<!-- k-wiki:end -->";

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

    let store = ProjectionStore::new(&repository_root);
    let state_root = store.state_root();
    let already_initialized = state_root.is_dir();
    store
        .initialize()
        .map_err(|_| "the .kwiki state directory could not be initialized".to_string())?;
    install_instruction_blocks(&repository_root)?;

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

fn install_instruction_blocks(repository_root: &Path) -> Result<(), String> {
    ["AGENTS.md", "CLAUDE.md"]
        .into_iter()
        .map(|file_name| repository_root.join(file_name))
        .try_for_each(|path| upsert_instruction_block(&path))
}

fn upsert_instruction_block(path: &Path) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let next = upsert_instruction_text(&existing, K_WIKI_WORKFLOW);
    if next == existing {
        return Ok(());
    }
    std::fs::write(path, next)
        .map_err(|error| format!("failed to write instructions {}: {error}", path.display()))
}

fn upsert_instruction_text(existing: &str, block: &str) -> String {
    if existing.trim().is_empty() {
        return block.to_string();
    }
    let Some(start) = existing.find(INSTRUCTION_START) else {
        return format!("{}\n\n{}", existing.trim_end(), block);
    };
    let Some(end) = existing[start..]
        .find(INSTRUCTION_END)
        .map(|offset| start + offset)
    else {
        return format!("{}\n\n{}", existing.trim_end(), block);
    };
    let after_end = end + INSTRUCTION_END.len();
    format!(
        "{}\n\n{}\n\n{}\n",
        existing[..start].trim_end(),
        block.trim_end(),
        existing[after_end..].trim_start(),
    )
    .trim()
    .to_string()
        + "\n"
}

const K_WIKI_WORKFLOW: &str = concat!(
    "<!-- k-wiki:start -->\n",
    "## k-wiki workflow\n",
    "- Use the `k_wiki` MCP server for every wiki interaction; do not invoke the `k-wiki` CLI or edit generated state directly.\n",
    "- Treat `knowledge/` as curated repository intent, not a substitute for current code. Start with `wiki_list_bundles`, then `wiki_search_concepts`; use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks`, and `wiki_get_neighborhood` to understand related decisions.\n",
    "- Use the wiki for architecture, terminology, invariants, ownership, and prior decisions. Verify changeable details with codebase-graph MCP tools. If code and wiki conflict, identify the conflict and use `wiki_populate_page` to record clarified intent.\n",
    "- Create missing pages with `wiki_create_page`; update existing pages with `wiki_populate_page`, supplying title, type, tags, useful Markdown, and `expected_content_hash`. Record durable decisions, public contracts, runbooks, invariants, and non-obvious trade-offs—not transient implementation noise or copied source.\n",
    "- After meaningful wiki edits, call `wiki_validate` with `profile: recommended` and `include_structured_content: true`, then `wiki_check_links`. Call `wiki_build` with the configured `bundle_root` and `.kwiki/site` output root; it is a write operation.\n",
    "- `knowledge/` is source and `.kwiki/` is generated state. Never manually edit generated projections.\n",
    "- Use `wiki_get_diagnostics` to inspect remaining issues and `wiki_get_recent_changes` to understand recent work. In handoffs, cite updated concept paths and summarize decisions, uncertainties, and validation results.\n",
    "<!-- k-wiki:end -->\n",
);

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

    let descriptor = McpServerDescriptor {
        name: request.name.unwrap_or_else(|| "k_wiki".to_string()),
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

#[cfg(test)]
mod tests {
    use super::{install_repository, InstallOutcome, INSTRUCTION_START};
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

    #[test]
    fn install_repository_upserts_mcp_only_workflow_in_both_instruction_files() {
        let root = unique_temp_dir("instructions");
        for file_name in ["AGENTS.md", "CLAUDE.md"] {
            let stale_workflow = if file_name == "CLAUDE.md" {
                "\n<!-- k-wiki:start -->\nstale workflow\n<!-- k-wiki:end -->\n"
            } else {
                ""
            };
            fs::write(
                root.join(file_name),
                format!(
                    "before\n\n<!-- codebaseGraph:start -->\ngraph instructions\n<!-- codebaseGraph:end -->\n\nafter{stale_workflow}"
                ),
            )
            .expect("seed instructions");
        }

        install_repository(&root).expect("install repository");
        let initial = ["AGENTS.md", "CLAUDE.md"].map(|file_name| {
            let text = fs::read_to_string(root.join(file_name)).expect("read instructions");
            assert!(text.contains("before"));
            assert!(text.contains("after"));
            assert!(text.contains("<!-- codebaseGraph:start -->"));
            assert!(text.contains(INSTRUCTION_START));
            assert!(text.contains("wiki_validate"));
            assert!(text.contains("wiki_check_links"));
            assert!(text.contains("wiki_build"));
            assert!(!text.contains("stale workflow"));
            assert_eq!(text.matches(INSTRUCTION_START).count(), 1);
            text
        });

        install_repository(&root).expect("rerun install repository");
        for (file_name, expected) in ["AGENTS.md", "CLAUDE.md"].into_iter().zip(initial) {
            assert_eq!(
                fs::read_to_string(root.join(file_name)).expect("read updated instructions"),
                expected
            );
        }

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
