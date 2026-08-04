use std::{collections::BTreeMap, sync::OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::WikiOperationRequest;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationSurface {
    Rust,
    Cli,
    Http,
    Mcp,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    Read,
    Write,
}

#[derive(Clone, Debug)]
pub struct OperationDescriptor {
    pub id: &'static str,
    pub summary: &'static str,
    pub surfaces: &'static [OperationSurface],
    pub access: AccessMode,
    pub mcp_tool_name: Option<&'static str>,
    pub mcp_display_name: Option<&'static str>,
    pub mcp_order: Option<u8>,
    pub request_schema: fn() -> Value,
}

const ALL_SURFACES: &[OperationSurface] = &[
    OperationSurface::Rust,
    OperationSurface::Cli,
    OperationSurface::Http,
    OperationSurface::Mcp,
];
const RUST_CLI_HTTP: &[OperationSurface] = &[
    OperationSurface::Rust,
    OperationSurface::Cli,
    OperationSurface::Http,
];

pub fn operation_descriptors() -> &'static BTreeMap<&'static str, OperationDescriptor> {
    static REGISTRY: OnceLock<BTreeMap<&'static str, OperationDescriptor>> = OnceLock::new();
    REGISTRY.get_or_init(build_registry)
}

pub fn operation_descriptor(id: &str) -> Option<&'static OperationDescriptor> {
    operation_descriptors().get(id)
}

pub fn mcp_operation_descriptors() -> Vec<&'static OperationDescriptor> {
    let mut descriptors = operation_descriptors()
        .values()
        .filter(|descriptor| descriptor.mcp_order.is_some())
        .collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.mcp_order);
    descriptors
}

pub fn mcp_operation_descriptor(tool_name: &str) -> Option<&'static OperationDescriptor> {
    mcp_operation_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.mcp_tool_name == Some(tool_name))
}

pub fn operation_id(request: &WikiOperationRequest) -> &'static str {
    match request {
        WikiOperationRequest::Health(_) => "health",
        WikiOperationRequest::ValidateBundle(_) => "validate_bundle",
        WikiOperationRequest::CheckLinks(_) => "check_links",
        WikiOperationRequest::CreateBundle(_) => "create_bundle",
        WikiOperationRequest::CreatePage(_) => "create_page",
        WikiOperationRequest::PopulatePage(_) => "populate_page",
        WikiOperationRequest::BuildProjection(_) => "build_projection",
        WikiOperationRequest::ListBundles(_) => "list_bundles",
        WikiOperationRequest::GetDirectory(_) => "get_directory",
        WikiOperationRequest::GetConcept(_) => "get_concept",
        WikiOperationRequest::SearchConcepts(_) => "search_concepts",
        WikiOperationRequest::GetBacklinks(_) => "get_backlinks",
        WikiOperationRequest::GetNeighborhood(_) => "get_neighborhood",
        WikiOperationRequest::GetDiagnostics(_) => "get_diagnostics",
        WikiOperationRequest::GetRecentChanges(_) => "get_recent_changes",
        WikiOperationRequest::BuildSite(_) => "build_site",
        WikiOperationRequest::RenderSite(_) => "render_site",
    }
}

fn build_registry() -> BTreeMap<&'static str, OperationDescriptor> {
    let mut registry = BTreeMap::new();
    let descriptors = [
        descriptor(
            "health",
            "Inspect wiki availability.",
            RUST_CLI_HTTP,
            AccessMode::Read,
            None,
            empty_schema,
        ),
        descriptor(
            "validate_bundle",
            "Validate one OKF bundle.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_validate", "Validate Bundle", 11)),
            validate_bundle_schema,
        ),
        descriptor(
            "check_links",
            "Check links in one OKF bundle.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_check_links", "Check Links", 12)),
            check_links_schema,
        ),
        descriptor(
            "create_bundle",
            "Create a conformant OKF bundle.",
            ALL_SURFACES,
            AccessMode::Write,
            Some(mcp("wiki_create_bundle", "Create Bundle", 8)),
            create_bundle_schema,
        ),
        descriptor(
            "create_page",
            "Create a concept page.",
            ALL_SURFACES,
            AccessMode::Write,
            Some(mcp("wiki_create_page", "Create Page", 9)),
            create_page_schema,
        ),
        descriptor(
            "populate_page",
            "Populate a concept page.",
            ALL_SURFACES,
            AccessMode::Write,
            Some(mcp("wiki_populate_page", "Populate Page", 10)),
            populate_page_schema,
        ),
        descriptor(
            "build_projection",
            "Compile normalized wiki projections.",
            RUST_CLI_HTTP,
            AccessMode::Write,
            None,
            build_projection_schema,
        ),
        descriptor(
            "list_bundles",
            "List configured OKF bundles.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_list_bundles", "List Bundles", 0)),
            list_bundles_schema,
        ),
        descriptor(
            "get_directory",
            "List a wiki directory.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_list_directory", "List Directory", 1)),
            directory_schema,
        ),
        descriptor(
            "get_concept",
            "Read one concept.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_get_concept", "Get Concept", 2)),
            concept_schema,
        ),
        descriptor(
            "search_concepts",
            "Search normalized concepts.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_search_concepts", "Search Concepts", 3)),
            search_schema,
        ),
        descriptor(
            "get_backlinks",
            "Read incoming concept links.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_get_backlinks", "Get Backlinks", 4)),
            concept_schema,
        ),
        descriptor(
            "get_neighborhood",
            "Read a bounded concept neighborhood.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_get_neighborhood", "Get Neighborhood", 5)),
            neighborhood_schema,
        ),
        descriptor(
            "get_diagnostics",
            "Read bundle diagnostics.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_get_diagnostics", "Get Diagnostics", 6)),
            diagnostics_schema,
        ),
        descriptor(
            "get_recent_changes",
            "Read scoped recent changes.",
            ALL_SURFACES,
            AccessMode::Read,
            Some(mcp("wiki_get_recent_changes", "Get Recent Changes", 7)),
            recent_changes_schema,
        ),
        descriptor(
            "render_site",
            "Render a static wiki site.",
            RUST_CLI_HTTP,
            AccessMode::Write,
            None,
            render_site_schema,
        ),
        descriptor(
            "build_site",
            "Build a static site from one OKF bundle.",
            ALL_SURFACES,
            AccessMode::Write,
            Some(mcp("wiki_build", "Build Site", 13)),
            build_site_schema,
        ),
    ];

    for item in descriptors {
        assert!(
            registry.insert(item.id, item).is_none(),
            "duplicate wiki operation id"
        );
    }
    registry
}

#[derive(Clone, Copy)]
struct McpMetadata {
    tool: &'static str,
    display_name: &'static str,
    order: u8,
}

const fn mcp(tool: &'static str, display_name: &'static str, order: u8) -> McpMetadata {
    McpMetadata {
        tool,
        display_name,
        order,
    }
}

fn descriptor(
    id: &'static str,
    summary: &'static str,
    surfaces: &'static [OperationSurface],
    access: AccessMode,
    mcp: Option<McpMetadata>,
    request_schema: fn() -> Value,
) -> OperationDescriptor {
    OperationDescriptor {
        id,
        summary,
        surfaces,
        access,
        mcp_tool_name: mcp.map(|metadata| metadata.tool),
        mcp_display_name: mcp.map(|metadata| metadata.display_name),
        mcp_order: mcp.map(|metadata| metadata.order),
        request_schema,
    }
}

fn empty_schema() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

fn validate_bundle_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_root": {"type": "string"},
            "profile": {"enum": ["consume", "conformant", "recommended"]}
        }),
        &["bundle_root", "profile"],
    )
}

fn check_links_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_root": {"type": "string"}
        }),
        &["bundle_root"],
    )
}

fn build_projection_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_roots": {"type": "array", "items": {"type": "string"}},
            "generated_at": {"type": "string"},
            "source_revision": {"type": ["string", "null"]}
        },
        "required": ["bundle_roots", "generated_at"],
        "additionalProperties": false
    })
}

fn create_bundle_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_id": {"type": "string"},
            "repository_id": {"type": "string"},
            "bundle_path": {"type": "string"},
            "okf_version": {"type": "string"},
            "title": {"type": ["string", "null"]},
            "body_markdown": {"type": ["string", "null"]},
            "include_structured_content": {"type": "boolean", "default": false}
        },
        "required": ["bundle_id", "repository_id", "bundle_path", "okf_version"],
        "additionalProperties": false
    })
}

fn create_page_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_id": {"type": "string"},
            "page_path": {"type": "string"},
            "type": {"type": "string"},
            "title": {"type": ["string", "null"]},
            "description": {"type": ["string", "null"]},
            "resource": {"type": ["string", "null"]},
            "tags": {"type": "array", "items": {"type": "string"}, "default": []},
            "timestamp": {"type": ["string", "null"]},
            "body_markdown": {"type": ["string", "null"]},
            "include_structured_content": {"type": "boolean", "default": false}
        },
        "required": ["bundle_id", "page_path", "type"],
        "additionalProperties": false
    })
}

fn populate_page_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_id": {"type": "string"},
            "page_path": {"type": "string"},
            "frontmatter": {
                "type": "object",
                "properties": {
                    "type": {"type": "string"},
                    "title": {"type": ["string", "null"]},
                    "description": {"type": ["string", "null"]},
                    "resource": {"type": ["string", "null"]},
                    "tags": {"type": "array", "items": {"type": "string"}, "default": []},
                    "timestamp": {"type": ["string", "null"]},
                    "extensions": {"type": "object", "default": {}}
                },
                "required": ["type"],
                "additionalProperties": false
            },
            "body_markdown": {"type": "string"},
            "expected_content_hash": {"type": ["string", "null"]},
            "include_structured_content": {"type": "boolean", "default": false}
        },
        "required": ["bundle_id", "page_path", "frontmatter", "body_markdown"],
        "additionalProperties": false
    })
}

fn list_bundles_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "include_structured_content": {"type": "boolean", "default": false}
        },
        "additionalProperties": false
    })
}

fn directory_schema() -> Value {
    schema_with_properties(
        json!({"bundle_id": {"type": "string"}, "path": {"type": "string"}}),
        &["bundle_id", "path"],
    )
}

fn concept_schema() -> Value {
    schema_with_properties(
        json!({"bundle_id": {"type": "string"}, "concept_id": {"type": "string"}}),
        &["bundle_id", "concept_id"],
    )
}

fn search_schema() -> Value {
    schema_with_properties(
        json!({
            "text": {"type": "string"},
            "bundle_id": {"type": ["string", "null"]},
            "concept_type": {"type": ["string", "null"]},
            "tags": {"type": "array", "items": {"type": "string"}, "default": []},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
        }),
        &["text"],
    )
}

fn neighborhood_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_id": {"type": "string"},
            "concept_id": {"type": "string"},
            "depth": {"type": "integer", "minimum": 1, "maximum": 3, "default": 1},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
        }),
        &["bundle_id", "concept_id"],
    )
}

fn diagnostics_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_id": {"type": "string"},
            "profile": {"enum": ["consume", "conformant", "recommended"]}
        }),
        &["bundle_id", "profile"],
    )
}

fn recent_changes_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_id": {"type": "string"},
            "path": {"type": ["string", "null"]},
            "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 50}
        }),
        &["bundle_id"],
    )
}

fn render_site_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_ids": {"type": "array", "items": {"type": "string"}},
            "output_root": {"type": "string"},
            "base_url": {"type": "string", "default": ""}
        },
        "required": ["bundle_ids", "output_root"],
        "additionalProperties": false
    })
}

fn build_site_schema() -> Value {
    schema_with_properties(
        json!({
            "bundle_root": {"type": "string"},
            "output_root": {"type": "string"},
            "base_url": {"type": "string", "default": ""}
        }),
        &["bundle_root", "output_root"],
    )
}

fn schema_with_properties(mut properties: Value, required: &[&str]) -> Value {
    if let Some(entries) = properties.as_object_mut() {
        entries.insert(
            "include_structured_content".to_string(),
            json!({"type": "boolean", "default": false}),
        );
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_sorted_unique_and_advertises_exact_mcp_surface() {
        let registry = operation_descriptors();
        let ids = registry.keys().copied().collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        assert_eq!(registry.len(), 17);

        let tools = mcp_operation_descriptors()
            .into_iter()
            .filter_map(|descriptor| descriptor.mcp_tool_name)
            .collect::<Vec<_>>();
        assert_eq!(
            tools,
            [
                "wiki_list_bundles",
                "wiki_list_directory",
                "wiki_get_concept",
                "wiki_search_concepts",
                "wiki_get_backlinks",
                "wiki_get_neighborhood",
                "wiki_get_diagnostics",
                "wiki_get_recent_changes",
                "wiki_create_bundle",
                "wiki_create_page",
                "wiki_populate_page",
                "wiki_validate",
                "wiki_check_links",
                "wiki_build",
            ]
        );
    }
}
