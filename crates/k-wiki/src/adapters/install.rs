use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use codebase_graph::api::{
    install_mcp_server, remove_mcp_server, resolve_mcp_target, McpClientInstallOptions,
    McpClientRemovalOptions, McpExistingEntryPolicy, McpInstallMode, McpServerDescriptor,
    McpTargetLocality, ResolvedMcpTarget,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    authoring::{
        AuthoringConfig, AuthoringService, ConformanceAuthoringValidator, CreateBundleRequest,
        NoopRefreshNotifier, RepositoryRoot,
    },
    projection::ProjectionStore,
};

const DEFAULT_BUNDLE_ID: &str = "knowledge";
const DEFAULT_BUNDLE_PATH: &str = "knowledge";
const DEFAULT_SERVER_NAME: &str = "k_wiki";
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
    install_instruction_blocks_with_servers(repository_root, &[])
}

fn install_instruction_blocks_with_servers(
    repository_root: &Path,
    server_names: &[String],
) -> Result<(), String> {
    ["AGENTS.md", "CLAUDE.md"]
        .into_iter()
        .map(|file_name| repository_root.join(file_name))
        .try_for_each(|path| upsert_instruction_block(&path, server_names))
}

fn upsert_instruction_block(path: &Path, server_names: &[String]) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let next = upsert_instruction_text(&existing, &k_wiki_workflow(server_names));
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

fn k_wiki_workflow(server_names: &[String]) -> String {
    let registration_line = if server_names.is_empty() {
        "- Use the configured k-wiki MCP server for wiki interaction; do not invoke the `k-wiki` CLI or edit generated state directly.".to_string()
    } else {
        format!(
            "- Use the configured k-wiki MCP server registration(s) for wiki interaction: {}; do not invoke the `k-wiki` CLI or edit generated state directly.",
            server_names
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{INSTRUCTION_START}\n## k-wiki workflow\n{registration_line}\n- Treat `knowledge/` as curated repository intent, not a substitute for current code. Start with `wiki_list_bundles`, then `wiki_search_concepts`; use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks`, and `wiki_get_neighborhood` to understand related decisions.\n- Use the wiki for architecture, terminology, invariants, ownership, and prior decisions. Verify changeable details with codebase-graph MCP tools. If code and wiki conflict, identify the conflict and use `wiki_populate_page` to record clarified intent.\n- Create missing pages with `wiki_create_page`; update existing pages with `wiki_populate_page`, supplying title, type, tags, useful Markdown, and `expected_content_hash`. Record durable decisions, public contracts, runbooks, invariants, and non-obvious trade-offs—not transient implementation noise or copied source.\n- After meaningful wiki edits, call `wiki_validate` with `profile: recommended` and `include_structured_content: true`, then `wiki_check_links`. Call `wiki_build` with the configured `bundle_root` and `.kwiki/site` output root; it is a write operation.\n- `knowledge/` is source and `.kwiki/` is generated state. Never manually edit generated projections.\n- Use `wiki_get_diagnostics` to inspect remaining issues and `wiki_get_recent_changes` to understand recent work. In handoffs, cite updated concept paths and summarize decisions, uncertainties, and validation results.\n{INSTRUCTION_END}\n"
    )
}

pub fn install_mcp_client(request: McpInstallRequest) -> Result<serde_json::Value, String> {
    let repository_root = request
        .repo_root
        .clone()
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

    let requested_client = request.client.trim().to_ascii_lowercase();
    let clients = if requested_client == "all" {
        codebase_graph::api::supported_mcp_clients()
            .iter()
            .copied()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        vec![request.client.trim().to_string()]
    };
    let mut results = Vec::new();
    let mut instruction_names = BTreeSet::new();
    for client in clients {
        match install_single_mcp_client(&repository_root, &bundle_root, &request, client.clone()) {
            Ok(result) => {
                if !request.dry_run {
                    if let Some(name) = result
                        .get("server_name")
                        .and_then(serde_json::Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                    {
                        instruction_names.insert(name.to_string());
                    }
                }
                results.push(result);
            }
            Err(error) if requested_client == "all" => {
                results.push(json!({
                    "action": "failed",
                    "client": client,
                    "scope": &request.scope,
                    "server_name": &request.name,
                    "method": "file_adapter",
                    "path": serde_json::Value::Null,
                    "target_locality": serde_json::Value::Null,
                    "legacy_cleanup": {"action": "not_run"},
                    "error": error,
                }));
            }
            Err(error) => return Err(error),
        }
    }

    if !request.dry_run && !instruction_names.is_empty() {
        install_instruction_blocks_with_servers(
            &repository_root,
            &instruction_names.into_iter().collect::<Vec<_>>(),
        )?;
    }

    if requested_client == "all" {
        Ok(json!({ "results": results }))
    } else {
        results
            .into_iter()
            .next()
            .ok_or_else(|| "no MCP install result was produced".to_string())
    }
}

fn install_single_mcp_client(
    repository_root: &Path,
    bundle_root: &Path,
    request: &McpInstallRequest,
    client: String,
) -> Result<serde_json::Value, String> {
    let normalized_scope = request.scope.trim().to_ascii_lowercase();
    let probe_descriptor = build_descriptor(
        DEFAULT_SERVER_NAME.to_string(),
        bundle_root,
        repository_root,
    );
    let resolved = resolve_mcp_target(
        &client,
        &normalized_scope,
        &probe_descriptor,
        request.client_config_path.clone(),
    )?;
    let server_name = resolve_server_name(request.name.as_deref(), &resolved, repository_root)?;
    let descriptor = build_descriptor(server_name.clone(), bundle_root, repository_root);
    let legacy_server_names = match resolved.locality {
        McpTargetLocality::Shared | McpTargetLocality::Manual => {
            vec![DEFAULT_SERVER_NAME.to_string()]
        }
        McpTargetLocality::RepositoryLocal => Vec::new(),
    };
    let mut payload = install_mcp_server(
        &descriptor,
        &McpClientInstallOptions {
            client: client.clone(),
            scope: normalized_scope.clone(),
            client_config_path: request.client_config_path.clone(),
            dry_run: request.dry_run,
            install_method: McpInstallMode::FileAdapter,
            existing_entry_policy: McpExistingEntryPolicy::RejectDifferent,
            legacy_server_names,
        },
    )?;
    if let Some(shared_cleanup) =
        remove_shared_legacy_registration(&resolved, &descriptor, request.dry_run)?
    {
        let mut legacy_cleanup = payload
            .get("legacy_cleanup")
            .cloned()
            .unwrap_or_else(|| json!({}));
        legacy_cleanup["shared_target"] = shared_cleanup;
        payload["legacy_cleanup"] = legacy_cleanup;
    }
    Ok(payload)
}

fn resolve_server_name(
    requested_name: Option<&str>,
    target: &ResolvedMcpTarget,
    repository_root: &Path,
) -> Result<String, String> {
    if let Some(name) = requested_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if matches!(
            target.locality,
            McpTargetLocality::Shared | McpTargetLocality::Manual
        ) && name == DEFAULT_SERVER_NAME
        {
            return Err(format!(
                "server name `{DEFAULT_SERVER_NAME}` is reserved for repository-local registrations"
            ));
        }
        return Ok(name.to_string());
    }
    Ok(match target.locality {
        McpTargetLocality::RepositoryLocal => DEFAULT_SERVER_NAME.to_string(),
        McpTargetLocality::Shared | McpTargetLocality::Manual => {
            format!(
                "{DEFAULT_SERVER_NAME}_{}_{}",
                sanitized_repo_name(repository_root),
                repository_hash(repository_root)
            )
        }
    })
}

fn build_descriptor(
    name: String,
    bundle_root: &Path,
    repository_root: &Path,
) -> McpServerDescriptor {
    McpServerDescriptor {
        name,
        command: std::env::var("K_WIKI_SERVER_COMMAND").unwrap_or_else(|_| "k-wiki".to_string()),
        args: vec!["mcp".to_string(), bundle_root.to_string_lossy().to_string()],
        repo_root: repository_root.to_path_buf(),
        timeout: 60,
        setup_config_path: None,
        tool_policy: Some("knowledge_wiki".to_string()),
        manual_http_metadata: None,
    }
}

fn remove_shared_legacy_registration(
    target: &ResolvedMcpTarget,
    descriptor: &McpServerDescriptor,
    dry_run: bool,
) -> Result<Option<serde_json::Value>, String> {
    if target.locality != McpTargetLocality::RepositoryLocal {
        return Ok(None);
    }
    let Some(shared_target) = shared_cleanup_target(target, descriptor)? else {
        return Ok(None);
    };
    if shared_target.path == target.path {
        return Ok(None);
    }
    remove_mcp_server(
        DEFAULT_SERVER_NAME,
        &McpClientRemovalOptions {
            target: shared_target,
            dry_run,
        },
    )
    .map(Some)
    .map_err(|error| {
        if dry_run {
            format!(
                "dry run could not inspect the shared legacy `{}` registration for {} {}: {error}. No files were changed",
                DEFAULT_SERVER_NAME, target.client, target.scope
            )
        } else {
            format!(
                "partial migration: installed `{}` for {} {} but failed to remove shared legacy `{}` registration: {error}. The new local registration was kept and not rolled back",
                descriptor.name, target.client, target.scope, DEFAULT_SERVER_NAME
            )
        }
    })
}

fn shared_cleanup_target(
    target: &ResolvedMcpTarget,
    descriptor: &McpServerDescriptor,
) -> Result<Option<ResolvedMcpTarget>, String> {
    match target.client.as_str() {
        "codex" => resolve_mcp_target("codex", "user", descriptor, None).map(Some),
        "claude" | "claude-project" => {
            resolve_mcp_target("claude", "local", descriptor, None).map(Some)
        }
        "generic" => resolve_mcp_target("generic", "local", descriptor, None).map(Some),
        "hermes" | "lmstudio" | "openclaw" => {
            resolve_mcp_target(&target.client, "user", descriptor, None).map(Some)
        }
        _ => Ok(None),
    }
}

fn sanitized_repo_name(repository_root: &Path) -> String {
    let source = repository_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repository");
    let normalized = source
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else if character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = normalized.trim_matches(['.', '_', '-']);
    if trimmed.is_empty() {
        "repository".to_string()
    } else {
        trimmed.to_string()
    }
}

fn repository_hash(repository_root: &Path) -> String {
    let normalized = repository_root.to_string_lossy().replace('\\', "/");
    let normalized = if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    };
    let digest = Sha256::digest(normalized.as_bytes());
    digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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
