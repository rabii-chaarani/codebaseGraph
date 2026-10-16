use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use k_wiki::{
    adapters::{http, mcp},
    api::{
        BuildProjectionRequest, CheckLinksRequest, GetConceptRequest, GetDiagnosticsRequest,
        HealthRequest, OkfWikiApi, ValidateBundleRequest, ValidationProfile,
        WikiOperationRequest as Request, WikiOperationResponse as Response,
    },
    authoring::{
        AuthoringConfig, AuthoringService, BundleRoot, ConformanceAuthoringValidator,
        NoopRefreshNotifier, RepositoryRoot,
    },
    diagnostic::Diagnostic,
    service::LocalWikiService,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    bundle: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "k-wiki-diagnostics-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let bundle = root.join("docs");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("index.md"),
            "---\nokf_version: '0.1'\ntitle: Docs\n---\n# Docs\n",
        )
        .unwrap();
        Self { root, bundle }
    }

    fn write(&self, path: &str, content: impl AsRef<[u8]>) {
        fs::write(self.bundle.join(path), content).unwrap();
    }

    fn api(&self) -> OkfWikiApi<LocalWikiService> {
        LocalWikiService::new(vec![self.bundle.clone()]).into_api()
    }

    fn authoring_api(&self) -> OkfWikiApi<LocalWikiService> {
        let authoring = AuthoringService::new(
            AuthoringConfig {
                repositories: vec![RepositoryRoot {
                    id: "repo".into(),
                    root_path: self.root.clone(),
                }],
                bundles: vec![BundleRoot {
                    id: "docs".into(),
                    repository_id: "repo".into(),
                    root_path: self.bundle.clone(),
                }],
            },
            ConformanceAuthoringValidator,
            NoopRefreshNotifier,
        )
        .unwrap();
        LocalWikiService::new(vec![self.bundle.clone()])
            .with_authoring(authoring)
            .into_api()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn document(body: &str) -> String {
    format!("---\ntype: concept\ntitle: Page\ndescription: Fixture\n---\n{body}")
}

fn diagnostics(response: Response) -> Vec<Diagnostic> {
    match response {
        Response::Diagnostics(diagnostics) => diagnostics,
        other => panic!("expected diagnostics, received {other:?}"),
    }
}

fn build(
    api: &OkfWikiApi<LocalWikiService>,
    roots: Vec<PathBuf>,
    generated_at: &str,
) -> k_wiki::api::ProjectionBuiltResponse {
    match api
        .execute_operation(&Request::BuildProjection(BuildProjectionRequest {
            bundle_roots: roots,
            generated_at: generated_at.into(),
            source_revision: None,
        }))
        .unwrap()
    {
        Response::ProjectionBuilt(result) => result,
        other => panic!("expected projection, received {other:?}"),
    }
}

fn concept_request() -> Request {
    Request::GetConcept(GetConceptRequest {
        bundle_id: "docs".into(),
        concept_id: "page".into(),
    })
}

#[test]
fn maintenance_operations_share_conformance_and_compiler_diagnostics() {
    let fixture = Fixture::new();
    fixture.write("page.md", document("# Page\n[Missing](missing.md)\n[Fragment](page.md#absent)\n[Unsafe](javascript:alert)\n"));
    fixture.write("invalid.md", "No required frontmatter\n");
    let api = fixture.api();
    let compiled = build(&api, vec![], "runtime");
    let expected = &compiled.projection.bundles[0].diagnostics;
    for code in ["missing_frontmatter", "broken_link", "rejected_link"] {
        assert!(
            expected.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {expected:?}"
        );
    }
    let checked = diagnostics(
        api.execute_operation(&Request::CheckLinks(CheckLinksRequest {
            bundle_root: fixture.bundle.clone(),
        }))
        .unwrap(),
    );
    for code in ["missing_frontmatter", "broken_link", "rejected_link"] {
        assert!(
            checked.iter().any(|diagnostic| diagnostic.code == code),
            "link check omitted {code}"
        );
    }
    for profile in [
        ValidationProfile::Consume,
        ValidationProfile::Conformant,
        ValidationProfile::Recommended,
    ] {
        let queried = diagnostics(
            api.execute_operation(&Request::GetDiagnostics(GetDiagnosticsRequest {
                bundle_id: "docs".into(),
                profile,
            }))
            .unwrap(),
        );
        assert_eq!(&queried, expected);
        let Response::Validation(validated) = api
            .execute_operation(&Request::ValidateBundle(ValidateBundleRequest {
                bundle_root: fixture.bundle.clone(),
                profile,
            }))
            .unwrap()
        else {
            panic!("expected validation")
        };
        assert_eq!(validated.diagnostics, queried);
        assert_eq!(validated.accepted, profile == ValidationProfile::Consume);
    }
    let mut deduplicated = expected.clone();
    deduplicated.dedup();
    assert_eq!(&deduplicated, expected);
    for (tool, arguments) in [
        (
            "wiki_check_links",
            json!({"bundle_root": fixture.bundle, "include_structured_content": true}),
        ),
        (
            "wiki_get_diagnostics",
            json!({"bundle_id":"docs", "profile":"conformant", "include_structured_content": true}),
        ),
    ] {
        let response = mcp_call(&api, tool, arguments);
        assert_eq!(
            response["result"]["structuredContent"]["result"],
            serde_json::to_value(expected).unwrap()
        );
    }
}

#[test]
fn unterminated_frontmatter_is_reported_by_validation_and_builds() {
    let fixture = Fixture::new();
    fixture.write(
        "page.md",
        "---\ntype: concept\ntitle: Page\ndescription: Fixture\n",
    );
    let api = fixture.api();
    for (profile, accepted) in [
        (ValidationProfile::Consume, true),
        (ValidationProfile::Conformant, false),
        (ValidationProfile::Recommended, false),
    ] {
        let Response::Validation(result) = api
            .execute_operation(&Request::ValidateBundle(ValidateBundleRequest {
                bundle_root: fixture.bundle.clone(),
                profile,
            }))
            .unwrap()
        else {
            panic!("expected validation")
        };
        assert_eq!(result.accepted, accepted);
        assert!(result.diagnostics.iter().any(|diagnostic| diagnostic.code
            == "invalid_frontmatter"
            && diagnostic.line == Some(1)));
    }
    assert!(build(&api, vec![], "runtime").projection.bundles[0]
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "invalid_frontmatter"));
}

#[test]
fn validation_profiles_apply_to_merged_link_warnings() {
    let fixture = Fixture::new();
    fixture.write("page.md", document("[Missing](missing.md)\n"));
    let api = fixture.api();
    for (profile, accepted) in [
        (ValidationProfile::Consume, true),
        (ValidationProfile::Conformant, true),
        (ValidationProfile::Recommended, false),
    ] {
        let Response::Validation(result) = api
            .execute_operation(&Request::ValidateBundle(ValidateBundleRequest {
                bundle_root: fixture.bundle.clone(),
                profile,
            }))
            .unwrap()
        else {
            panic!("expected validation")
        };
        assert!(result.diagnostics.iter().any(|d| d.code == "broken_link"));
        assert_eq!(result.accepted, accepted);
    }
}

#[test]
fn compiler_locations_include_frontmatter_once_and_preserve_conformance_lines() {
    for newline in ["\n", "\r\n"] {
        for extra_fields in ["", "tags: [one, two]\nextra: retained\n"] {
            let fixture = Fixture::new();
            let source = document("# Page\n[Missing](missing.md)\n")
                .replacen("---\n# Page", &format!("{extra_fields}---\n# Page"), 1)
                .replace('\n', newline);
            let expected_line = source
                .lines()
                .position(|line| line.contains("[Missing]"))
                .unwrap()
                + 1;
            fixture.write("page.md", &source);
            fixture.write(
                "log.md",
                "---\ncustom: value\n---\n# Log\n1. Undated entry\n".replace('\n', newline),
            );
            fixture.write(
                "missing-type.md",
                "---\ntitle: Missing Type\ndescription: Fixture\n---\nBody\n"
                    .replace('\n', newline),
            );
            let api = fixture.api();
            for _ in 0..3 {
                api.execute_operation(&concept_request()).unwrap();
                let result = build(&api, vec![], "runtime");
                let diagnostics = &result.projection.bundles[0].diagnostics;
                let line = |code: &str, path: &str| {
                    diagnostics
                        .iter()
                        .find(|d| d.code == code && d.source_path == path)
                        .unwrap()
                        .line
                };
                assert_eq!(line("broken_link", "page.md"), Some(expected_line));
                assert_eq!(line("invalid_log_entry", "log.md"), Some(5));
                assert_eq!(line("reserved_frontmatter", "log.md"), Some(2));
                assert_eq!(line("missing_type", "missing-type.md"), Some(1));
            }
        }
    }
}

#[test]
fn failed_acquisition_cannot_replace_cache_even_via_permissive_build() {
    let fixture = Fixture::new();
    let original = document("# Original\n");
    fixture.write("page.md", &original);
    let api = fixture.api();
    let before = api.execute_operation(&concept_request()).unwrap();
    fixture.write("page.md", [0xff, 0xfe]);
    let error = api
        .execute_operation(&concept_request())
        .expect_err("incomplete refresh must fail");
    assert_eq!(error.code, "source_unavailable");
    let partial = build(&api, vec![], "runtime");
    assert!(!partial.cache_hit);
    assert!(partial.projection.bundles[0]
        .diagnostics
        .iter()
        .any(|d| d.code == "invalid_utf8"));
    let Response::Health(health) = api
        .execute_operation(&Request::Health(HealthRequest {}))
        .unwrap()
    else {
        panic!("expected health")
    };
    assert!(!health.ok);
    assert!(health.projection_available);
    fixture.write("page.md", original);
    assert!(
        build(&api, vec![], "runtime").cache_hit,
        "partial build replaced the last good snapshot"
    );
    assert_eq!(api.execute_operation(&concept_request()).unwrap(), before);
    let Response::Health(health) = api
        .execute_operation(&Request::Health(HealthRequest {}))
        .unwrap()
    else {
        panic!("expected health")
    };
    assert!(health.ok);
}

#[test]
fn external_edits_and_deletions_are_visible_on_the_next_read() {
    let fixture = Fixture::new();
    fixture.write("page.md", document("# Original\n"));
    let api = fixture.api();
    let before = api.execute_operation(&concept_request()).unwrap();
    fixture.write("page.md", document("# Modified\n"));
    let after = api.execute_operation(&concept_request()).unwrap();
    assert_ne!(before, after);
    fs::remove_file(fixture.bundle.join("page.md")).unwrap();
    assert_eq!(
        api.execute_operation(&concept_request()).unwrap_err().code,
        "concept_not_found"
    );
}

#[test]
fn unchanged_reads_reuse_the_explicit_projection_generation() {
    let fixture = Fixture::new();
    fixture.write("page.md", document("# Original\n"));
    let api = fixture.api();
    assert!(!build(&api, vec![], "explicit-generation").cache_hit);
    for _ in 0..3 {
        api.execute_operation(&concept_request()).unwrap();
    }
    assert!(build(&api, vec![], "explicit-generation").cache_hit);
}

#[test]
fn explicit_builds_for_other_roots_do_not_change_configured_reads() {
    let fixture = Fixture::new();
    fixture.write("page.md", document("# Original\n"));
    let api = fixture.api();
    let before = api.execute_operation(&concept_request()).unwrap();
    let other = fixture.root.join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("index.md"), "# Other\n").unwrap();
    build(&api, vec![other], "other");
    assert_eq!(api.execute_operation(&concept_request()).unwrap(), before);
}

fn mcp_call(api: &OkfWikiApi<LocalWikiService>, name: &str, arguments: Value) -> Value {
    mcp::handle_message(
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        &mut mcp::McpSession { protocol_version: Some(mcp::protocol_version().into()), initialized: true },
        &mut |_| json!({}),
        &mut |tool, arguments| mcp::dispatch_api(api, tool, arguments),
    ).unwrap()
}

struct HttpServer(tokio::task::JoinHandle<()>, std::net::SocketAddr);

impl HttpServer {
    async fn new(api: OkfWikiApi<LocalWikiService>, site: PathBuf) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, http::preview_router(api, site))
                .await
                .unwrap();
        });
        Self(handle, address)
    }

    async fn request(&self, request: Option<&Request>) -> (u16, Value) {
        let mut stream = tokio::net::TcpStream::connect(self.1).await.unwrap();
        let body = request
            .map(|request| serde_json::to_string(request).unwrap())
            .unwrap_or_default();
        let (method, path) = if request.is_some() {
            ("POST", "/api")
        } else {
            ("GET", "/healthz")
        };
        let message = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        stream.write_all(message.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str(body).unwrap())
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn malformed_source_errors_are_safe_through_api_mcp_and_http() {
    let fixture = Fixture::new();
    let malformed = "---\nSOURCE_CONTENT_MARKER\n---\nBody\n";
    fixture.write("page.md", malformed);
    let request = Request::PopulatePage(k_wiki::authoring::PopulatePageRequest {
        bundle_id: "docs".into(),
        page_path: "page.md".into(),
        frontmatter: k_wiki::authoring::PageFrontmatter {
            concept_type: "concept".into(),
            ..Default::default()
        },
        body_markdown: "Replacement".into(),
        expected_content_hash: Some(format!("{:x}", Sha256::digest(malformed.as_bytes()))),
    });
    let api = fixture.authoring_api();
    let error = api.execute_operation(&request).unwrap_err();
    assert_eq!(error.code, "invalid_frontmatter");
    assert!(!serde_json::to_string(&error)
        .unwrap()
        .contains("SOURCE_CONTENT_MARKER"));
    let Request::PopulatePage(ref populate) = request else {
        unreachable!()
    };
    let wire = mcp_call(
        &api,
        "wiki_populate_page",
        serde_json::to_value(populate).unwrap(),
    );
    assert_eq!(wire["error"]["data"]["code"], "invalid_frontmatter");
    assert!(!wire.to_string().contains("SOURCE_CONTENT_MARKER"));
    let server = HttpServer::new(fixture.authoring_api(), fixture.root.join("site")).await;
    let (status, response) = server.request(Some(&request)).await;
    assert_eq!(status, 400);
    assert_eq!(response["error"]["code"], "invalid_frontmatter");
    assert!(!response.to_string().contains("SOURCE_CONTENT_MARKER"));
    let (status, response) = server.request(None).await;
    assert_eq!(status, 503);
    assert_eq!(response["result"]["ok"], false);
}

#[cfg(unix)]
#[tokio::test]
async fn incomplete_scan_messages_are_safe_through_api_mcp_and_http() {
    use std::os::unix::fs::PermissionsExt;
    struct RestorePermissions(PathBuf, fs::Permissions);
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, self.1.clone());
        }
    }
    let fixture = Fixture::new();
    let locked = fixture.bundle.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("hidden.md"), document("Hidden")).unwrap();
    let _restore = RestorePermissions(locked.clone(), fs::metadata(&locked).unwrap().permissions());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        return;
    }
    let api = fixture.api();
    let request = Request::GetDiagnostics(GetDiagnosticsRequest {
        bundle_id: "docs".into(),
        profile: ValidationProfile::Conformant,
    });
    let response = api.execute_operation(&request).unwrap();
    let diagnostics = diagnostics(response.clone());
    assert!(diagnostics
        .iter()
        .any(|d| d.code == "io_read_failed" && d.source_path == "locked"));
    assert!(!serde_json::to_string(&response)
        .unwrap()
        .contains(fixture.root.to_str().unwrap()));
    let wire = mcp_call(
        &api,
        "wiki_get_diagnostics",
        json!({"bundle_id":"docs","profile":"conformant"}),
    );
    assert!(wire.to_string().contains("io_read_failed"));
    assert!(!wire.to_string().contains(fixture.root.to_str().unwrap()));
    let server = HttpServer::new(fixture.api(), fixture.root.join("site")).await;
    let (status, response) = server.request(Some(&request)).await;
    assert_eq!(status, 200);
    assert!(response.to_string().contains("io_read_failed"));
    assert!(!response
        .to_string()
        .contains(fixture.root.to_str().unwrap()));
    let (status, response) = server.request(None).await;
    assert_eq!(status, 503);
    assert_eq!(response["result"]["ok"], false);
}
