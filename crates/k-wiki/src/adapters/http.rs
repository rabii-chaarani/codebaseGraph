use super::TransportError;
use axum::{
    body::Body,
    extract::State,
    http::{
        header::{
            CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
            X_FRAME_OPTIONS,
        },
        HeaderMap, HeaderName, HeaderValue, Response as HttpResponse, StatusCode, Uri,
    },
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::api::{HealthRequest, OkfWikiApi, WikiOperationExecutor, WikiOperationRequest};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpServeOptions {
    pub host: String,
    pub port: u16,
}

impl Default for HttpServeOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4321,
        }
    }
}

impl HttpServeOptions {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--host" => {
                    options.host = required_value(args, index, "--host")?.to_string();
                    index += 2;
                }
                "--port" => {
                    options.port = required_value(args, index, "--port")?
                        .parse()
                        .map_err(|_| "--port must be between 0 and 65535".to_string())?;
                    index += 2;
                }
                other => return Err(format!("unknown serve option: {other}")),
            }
        }
        options.validate()?;
        Ok(options)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !is_local_host(&self.host) {
            return Err(
                "remote binding requires a separate approved security change; use localhost only"
                    .to_string(),
            );
        }
        Ok(())
    }
}

pub fn is_local_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

pub fn security_headers() -> [(HeaderName, HeaderValue); 5] {
    [
        (
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
            ),
        ),
        (X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        (X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
        (CACHE_CONTROL, HeaderValue::from_static("no-store")),
    ]
}

pub fn apply_security_headers(headers: &mut HeaderMap) {
    for (name, value) in security_headers() {
        headers.insert(name, value);
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/healthz", get(|| async move { StatusCode::NO_CONTENT }))
        .layer(middleware::map_response(add_security_headers))
}

/// Builds the localhost preview and JSON API router around one public API facade.
///
/// Each API or health request crosses the facade exactly once. Static files are
/// served only from the already-rendered site root and never access source
/// bundle paths.
pub fn preview_router<E>(api: OkfWikiApi<E>, site_root: PathBuf) -> Router
where
    E: WikiOperationExecutor + Send + Sync + 'static,
{
    let state = PreviewState {
        api: Arc::new(api),
        site_root: Arc::new(site_root),
    };
    Router::new()
        .route("/healthz", get(api_health::<E>))
        .route("/api", post(api_operation::<E>))
        .fallback(get(serve_static::<E>))
        .layer(middleware::map_response(add_security_headers))
        .with_state(state)
}

pub fn safe_error_response(error: &TransportError) -> (StatusCode, Json<Value>) {
    (
        status_for_error_code(&error.code),
        Json(json!({
            "error": {
                "code": error.code,
                "message": redact_absolute_paths(&error.message),
                "details": error.details.as_ref().map(sanitize_json_value),
                "retryable": error.retryable,
            }
        })),
    )
}

pub fn status_for_error_code(code: &str) -> StatusCode {
    match code {
        "invalid_request" | "invalid_frontmatter" => StatusCode::BAD_REQUEST,
        "bundle_not_found" | "concept_not_found" => StatusCode::NOT_FOUND,
        "bundle_exists" | "concept_exists" | "write_conflict" | "build_in_progress" => {
            StatusCode::CONFLICT
        }
        "projection_unavailable" | "graph_context_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "render_failed" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

struct PreviewState<E> {
    api: Arc<OkfWikiApi<E>>,
    site_root: Arc<PathBuf>,
}

impl<E> Clone for PreviewState<E> {
    fn clone(&self) -> Self {
        Self {
            api: Arc::clone(&self.api),
            site_root: Arc::clone(&self.site_root),
        }
    }
}

async fn api_health<E>(State(state): State<PreviewState<E>>) -> Response
where
    E: WikiOperationExecutor + Send + Sync + 'static,
{
    execute_api(
        state.api.as_ref(),
        WikiOperationRequest::Health(HealthRequest::default()),
    )
}

async fn api_operation<E>(
    State(state): State<PreviewState<E>>,
    Json(request): Json<WikiOperationRequest>,
) -> Response
where
    E: WikiOperationExecutor + Send + Sync + 'static,
{
    execute_api(state.api.as_ref(), request)
}

fn execute_api<E>(api: &OkfWikiApi<E>, request: WikiOperationRequest) -> Response
where
    E: WikiOperationExecutor,
{
    match api.execute_operation(&request) {
        Ok(response) => Json(response).into_response(),
        Err(error) => {
            let transport_error = TransportError {
                code: error.code,
                message: error.message,
                details: error.details,
                retryable: error.retryable,
            };
            safe_error_response(&transport_error).into_response()
        }
    }
}

async fn serve_static<E>(State(state): State<PreviewState<E>>, uri: Uri) -> Response
where
    E: WikiOperationExecutor + Send + Sync + 'static,
{
    let request_path = uri.path().trim_start_matches('/');
    let relative = if request_path.is_empty() {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(request_path)
    };
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut target = state.site_root.join(&relative);
    if !request_path.is_empty() && (target.is_dir() || uri.path().ends_with('/')) {
        target.push("index.html");
    }
    let Ok(canonical_root) = fs::canonicalize(state.site_root.as_ref()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(canonical_target) = fs::canonicalize(&target) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !canonical_target.starts_with(&canonical_root) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(bytes) = fs::read(&canonical_target) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let content_type = content_type(&canonical_target);
    HttpResponse::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "text/html; charset=utf-8",
    }
}

async fn add_security_headers(mut response: Response) -> Response {
    apply_security_headers(response.headers_mut());
    response
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn sanitize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sanitize_json_value).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), sanitize_json_value(value)))
                .collect(),
        ),
        Value::String(text) => Value::String(redact_absolute_paths(text)),
        _ => value.clone(),
    }
}

fn redact_absolute_paths(input: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if starts_with_url(&chars, index) {
            while index < chars.len()
                && !chars[index].is_whitespace()
                && !matches!(chars[index], '"' | '\'' | ')' | ']' | '}' | ',' | ';')
            {
                output.push(chars[index]);
                index += 1;
            }
        } else if chars[index] == '/' {
            output.push_str("[redacted-path]");
            index += 1;
            while index < chars.len()
                && !chars[index].is_whitespace()
                && !matches!(chars[index], '"' | '\'' | ')' | ']' | '}' | ',' | ';')
            {
                index += 1;
            }
        } else {
            output.push(chars[index]);
            index += 1;
        }
    }
    output
}

fn starts_with_url(chars: &[char], index: usize) -> bool {
    matches_char_slice(chars, index, "http://") || matches_char_slice(chars, index, "https://")
}

fn matches_char_slice(chars: &[char], index: usize, needle: &str) -> bool {
    let needle_chars: Vec<char> = needle.chars().collect();
    chars.get(index..index + needle_chars.len()) == Some(needle_chars.as_slice())
}
