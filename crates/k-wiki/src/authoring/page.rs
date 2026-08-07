use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use yaml_serde::Value as YamlValue;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateBundleRequest {
    pub bundle_id: String,
    pub repository_id: String,
    pub bundle_path: String,
    pub okf_version: String,
    pub title: Option<String>,
    pub body_markdown: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateBundleResult {
    pub bundle_id: String,
    pub repository_id: String,
    pub bundle_path: String,
    pub index_path: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreatePageRequest {
    pub bundle_id: String,
    pub page_path: String,
    #[serde(rename = "type")]
    pub concept_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resource: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub timestamp: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, YamlValue>,
    pub body_markdown: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreatePageResult {
    pub bundle_id: String,
    pub source_path: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageFrontmatter {
    #[serde(rename = "type")]
    pub concept_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub resource: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub timestamp: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, YamlValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PopulatePageRequest {
    pub bundle_id: String,
    pub page_path: String,
    pub frontmatter: PageFrontmatter,
    pub body_markdown: String,
    pub expected_content_hash: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PopulatePageResult {
    pub bundle_id: String,
    pub source_path: String,
    pub content_hash: String,
}
