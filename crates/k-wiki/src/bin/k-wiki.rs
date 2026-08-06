use std::{
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use k_wiki::{
    adapters::{
        cli::{self, CliRequest},
        http, install, mcp, TransportError, TransportPayload,
    },
    api::{
        mcp_operation_descriptor, BuildSiteRequest, CheckLinksRequest, GetConceptRequest,
        OkfWikiApi, RenderSiteRequest, ValidateBundleRequest, ValidationProfile, WikiApiError,
        WikiOperationRequest, WikiOperationResponse,
    },
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, ConformanceAuthoringValidator,
        NoopRefreshNotifier, RepositoryRoot,
    },
    bundle::load_bundle,
    diagnostic::DiagnosticSeverity,
    service::LocalWikiService,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|argument| argument == "mcp")
        && args.get(1).is_none_or(|argument| argument != "install")
    {
        let bundle = match args.get(1) {
            Some(path) if args.len() == 2 => PathBuf::from(path),
            None => PathBuf::from("."),
            _ => {
                eprintln!("usage: k-wiki mcp [bundle]");
                std::process::exit(2);
            }
        };
        if let Err(error) = serve_mcp(&bundle) {
            eprintln!("{}: {}", error.code, error.message);
            std::process::exit(1);
        }
        return;
    }

    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let exit_code = match cli::run(&args, &mut stdout, &mut stderr, dispatch_cli) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            2
        }
    };

    std::process::exit(exit_code);
}

fn serve_mcp(bundle: &Path) -> Result<(), TransportError> {
    let api = api_for_mcp_bundle(bundle)?;
    let mut session = mcp::McpSession::default();
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line
            .map_err(|_| TransportError::new("invalid_request", "MCP input could not be read"))?;
        if line.trim().is_empty() {
            continue;
        }
        let message = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(_) => {
                writeln!(
                    stdout,
                    "{}",
                    mcp::rpc_error(serde_json::Value::Null, -32700, "Invalid JSON")
                )
                .map_err(|_| {
                    TransportError::new("invalid_request", "MCP response could not be written")
                })?;
                stdout.flush().map_err(|_| {
                    TransportError::new("invalid_request", "MCP response could not be written")
                })?;
                continue;
            }
        };
        if let Some(response) = mcp::handle_message(
            message,
            &mut session,
            &mut |tool_name| {
                mcp_operation_descriptor(tool_name)
                    .map(|descriptor| (descriptor.request_schema)())
                    .unwrap_or_else(|| serde_json::json!({"type": "object"}))
            },
            &mut |tool_name, arguments| mcp::dispatch_api(&api, tool_name, arguments),
        ) {
            writeln!(stdout, "{response}").map_err(|_| {
                TransportError::new("invalid_request", "MCP response could not be written")
            })?;
            stdout.flush().map_err(|_| {
                TransportError::new("invalid_request", "MCP response could not be written")
            })?;
        }
    }
    Ok(())
}

fn dispatch_cli(request: CliRequest) -> Result<TransportPayload, TransportError> {
    match request {
        CliRequest::Install { repo_root } => {
            let repo_root = repo_root.unwrap_or_else(|| PathBuf::from("."));
            let outcome = install::install_repository(&repo_root)
                .map_err(|message| TransportError::new("installation_failed", message))?;
            let (verb, state_root, bundle_root) = match outcome {
                install::InstallOutcome::Initialized {
                    state_root,
                    bundle_root,
                } => ("initialized", state_root, bundle_root),
                install::InstallOutcome::AlreadyInitialized {
                    state_root,
                    bundle_root,
                } => ("already initialized", state_root, bundle_root),
            };
            Ok(TransportPayload::text(format!(
                "k-wiki repository state {verb} at {}; source bundle: {}.",
                state_root.display(),
                bundle_root.display()
            )))
        }
        CliRequest::InstallMcp { request } => {
            let payload = install::install_mcp_client(request)
                .map_err(|message| TransportError::new("mcp_installation_failed", message))?;
            if install::has_partial_migration_failure(&payload) {
                return Err(TransportError::new(
                    "mcp_installation_partial_migration",
                    "the repository-local registration was installed, but legacy migration did not complete",
                )
                .with_details(payload));
            }
            if install::has_verification_failure(&payload) {
                return Err(TransportError::new(
                    "mcp_installation_unverified",
                    "k-wiki MCP registration was installed but runtime verification failed",
                )
                .with_details(payload));
            }
            Ok(TransportPayload::structured(
                "k-wiki MCP client registration completed",
                payload,
            ))
        }
        CliRequest::Validate {
            bundle,
            profile,
            json,
        } => {
            let api = api_for_bundle(&bundle)?;
            let response = execute(
                &api,
                WikiOperationRequest::ValidateBundle(ValidateBundleRequest {
                    bundle_root: bundle,
                    profile: validation_profile(profile),
                }),
            )?;
            if json {
                response_payload("validation complete", response)
            } else if let WikiOperationResponse::Validation(result) = response {
                let errors = result
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
                    .count();
                Ok(TransportPayload::structured(
                    format!(
                        "{} ({} diagnostics, {errors} errors)",
                        if result.accepted {
                            "bundle accepted"
                        } else {
                            "bundle rejected"
                        },
                        result.diagnostics.len()
                    ),
                    serde_json::to_value(&result).map_err(serialization_error)?,
                ))
            } else {
                Err(unexpected_response())
            }
        }
        CliRequest::Build {
            bundle,
            out,
            base_url,
        } => {
            let api = api_for_bundle(&bundle)?;
            let response = execute(
                &api,
                WikiOperationRequest::BuildSite(BuildSiteRequest {
                    bundle_root: bundle,
                    output_root: out,
                    base_url: base_url.unwrap_or_default(),
                }),
            )?;
            response_payload("site rendered", response)
        }
        CliRequest::Serve { bundle, options } => serve(bundle, options),
        CliRequest::Inspect { bundle, concept_id } => {
            let (api, bundle_id) = api_and_bundle_id(&bundle)?;
            let response = execute(
                &api,
                WikiOperationRequest::GetConcept(GetConceptRequest {
                    bundle_id,
                    concept_id,
                }),
            )?;
            response_payload("concept loaded", response)
        }
        CliRequest::CheckLinks {
            bundle,
            include_external: _,
        } => {
            let api = api_for_bundle(&bundle)?;
            let response = execute(
                &api,
                WikiOperationRequest::CheckLinks(CheckLinksRequest {
                    bundle_root: bundle,
                }),
            )?;
            response_payload("link check complete", response)
        }
    }
}

fn serve(
    bundle: PathBuf,
    options: http::HttpServeOptions,
) -> Result<TransportPayload, TransportError> {
    options
        .validate()
        .map_err(|message| TransportError::new("invalid_request", message))?;
    let (api, bundle_id) = api_and_bundle_id(&bundle)?;
    let output_root = preview_output_root(&bundle);
    execute(
        &api,
        WikiOperationRequest::RenderSite(RenderSiteRequest {
            bundle_ids: vec![bundle_id],
            output_root: output_root.clone(),
            base_url: String::new(),
        }),
    )?;

    let address = socket_address(&options)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|_| TransportError::new("invalid_request", "preview runtime could not start"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(address).await.map_err(|_| {
            TransportError::new("invalid_request", "preview address is unavailable")
        })?;
        eprintln!(
            "Knowledge Wiki preview: http://{}",
            listener.local_addr().unwrap_or(address)
        );
        axum::serve(listener, http::preview_router(api, output_root))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .map_err(|_| {
                TransportError::new("invalid_request", "preview server stopped unexpectedly")
            })
    })?;

    Ok(TransportPayload::text("preview stopped"))
}

fn api_for_bundle(bundle: &Path) -> Result<OkfWikiApi<LocalWikiService>, TransportError> {
    Ok(LocalWikiService::new(vec![bundle.to_path_buf()]).into_api())
}

fn api_for_mcp_bundle(bundle: &Path) -> Result<OkfWikiApi<LocalWikiService>, TransportError> {
    let loaded = load_bundle(bundle)
        .map_err(|_| TransportError::new("bundle_not_found", "bundle could not be loaded"))?;
    let bundle_root = bundle
        .canonicalize()
        .map_err(|_| TransportError::new("bundle_not_found", "bundle could not be loaded"))?;
    let repository_root = bundle_root
        .parent()
        .ok_or_else(|| TransportError::new("invalid_request", "bundle must have a parent"))?
        .to_path_buf();
    let authoring = AuthoringService::new(
        AuthoringConfig {
            repositories: vec![RepositoryRoot {
                id: "repository".to_string(),
                root_path: repository_root,
            }],
            bundles: vec![BundleRoot {
                id: loaded.id,
                repository_id: "repository".to_string(),
                root_path: bundle_root.clone(),
            }],
        },
        ConformanceAuthoringValidator,
        NoopRefreshNotifier,
    )
    .map_err(|_| {
        TransportError::new(
            "invalid_request",
            "bundle authoring could not be configured",
        )
    })?;
    Ok(LocalWikiService::new(vec![bundle_root])
        .with_authoring(authoring)
        .into_api())
}

fn api_and_bundle_id(
    bundle: &Path,
) -> Result<(OkfWikiApi<LocalWikiService>, String), TransportError> {
    let loaded = load_bundle(bundle)
        .map_err(|_| TransportError::new("bundle_not_found", "bundle could not be loaded"))?;
    Ok((api_for_bundle(bundle)?, loaded.id))
}

fn execute(
    api: &OkfWikiApi<LocalWikiService>,
    request: WikiOperationRequest,
) -> Result<WikiOperationResponse, TransportError> {
    api.execute_operation(&request).map_err(transport_error)
}

fn response_payload(
    summary: &str,
    response: WikiOperationResponse,
) -> Result<TransportPayload, TransportError> {
    let structured = serde_json::to_value(response).map_err(serialization_error)?;
    Ok(TransportPayload::structured(summary, structured))
}

fn validation_profile(profile: cli::ValidationProfile) -> ValidationProfile {
    match profile {
        cli::ValidationProfile::Consume => ValidationProfile::Consume,
        cli::ValidationProfile::Conformant => ValidationProfile::Conformant,
        cli::ValidationProfile::Recommended => ValidationProfile::Recommended,
    }
}

fn preview_output_root(bundle: &Path) -> PathBuf {
    bundle
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".kwiki")
        .join("site")
}

fn socket_address(options: &http::HttpServeOptions) -> Result<SocketAddr, TransportError> {
    let address = match options.host.as_str() {
        "localhost" | "127.0.0.1" => SocketAddr::from(([127, 0, 0, 1], options.port)),
        "::1" => SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], options.port)),
        _ => {
            return Err(TransportError::new(
                "invalid_request",
                "remote binding is disabled",
            ))
        }
    };
    Ok(address)
}

fn transport_error(error: WikiApiError) -> TransportError {
    TransportError {
        code: error.code,
        message: error.message,
        details: error.details,
        retryable: error.retryable,
    }
}

fn serialization_error(_error: serde_json::Error) -> TransportError {
    TransportError::new("invalid_request", "response could not be serialized")
}

fn unexpected_response() -> TransportError {
    TransportError::new("invalid_request", "wiki returned an unexpected response")
}
