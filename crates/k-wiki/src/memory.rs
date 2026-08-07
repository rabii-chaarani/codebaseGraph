//! Repository-scoped durable memory stored as typed OKF concepts.

use serde::{Deserialize, Serialize};
use yaml_serde::Value as YamlValue;

use crate::model::Concept;

pub const AGENT_MEMORY_TYPE: &str = "agent-memory";
pub const AGENT_MEMORY_EXTENSION: &str = "agent_memory";
pub const AGENT_MEMORY_VERSION: u32 = 1;
pub const REPOSITORY_MEMORY_SCOPE: &str = "repository";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Semantic,
    Episodic,
    Procedural,
}

impl MemoryKind {
    pub const fn path_segment(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Episodic => "episodic",
            Self::Procedural => "procedural",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Candidate,
    Active,
    Superseded,
    Quarantined,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySource {
    pub kind: String,
    pub reference: String,
    pub content_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTransition {
    pub from: MemoryStatus,
    pub to: MemoryStatus,
    pub actor: String,
    pub at: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMemoryMetadata {
    pub version: u32,
    pub kind: MemoryKind,
    pub scope: String,
    pub status: MemoryStatus,
    pub owner: String,
    pub created_at: String,
    pub last_verified_at: Option<String>,
    pub verified_by: Option<String>,
    pub review_after: Option<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
    pub superseded_by: Option<String>,
    pub sources: Vec<MemorySource>,
    #[serde(default)]
    pub history: Vec<MemoryTransition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordMemoryRequest {
    pub bundle_id: String,
    pub memory_id: String,
    pub kind: MemoryKind,
    pub title: String,
    pub description: Option<String>,
    pub body_markdown: String,
    pub owner: String,
    pub created_at: String,
    pub sources: Vec<MemorySource>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub review_after: Option<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecallMemoryRequest {
    pub bundle_id: String,
    pub text: String,
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    #[serde(default = "default_recall_limit")]
    pub limit: usize,
}

const fn default_recall_limit() -> usize {
    20
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionMemoryRequest {
    pub bundle_id: String,
    pub memory_id: String,
    pub to_status: MemoryStatus,
    pub actor: String,
    pub transitioned_at: String,
    pub reason: String,
    pub replacement_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRecord {
    pub bundle_id: String,
    pub memory_id: String,
    pub concept_id: String,
    pub source_path: String,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub title: String,
    pub description: Option<String>,
    pub body_markdown: String,
    pub owner: String,
    pub created_at: String,
    pub last_verified_at: Option<String>,
    pub verified_by: Option<String>,
    pub review_after: Option<String>,
    pub supersedes: Vec<String>,
    pub superseded_by: Option<String>,
    pub sources: Vec<MemorySource>,
    pub history: Vec<MemoryTransition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRecallResult {
    pub bundle_id: String,
    pub memory_id: String,
    pub concept_id: String,
    pub source_path: String,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub title: String,
    pub description: Option<String>,
    pub body_markdown: String,
    pub owner: String,
    pub created_at: String,
    pub last_verified_at: Option<String>,
    pub verified_by: Option<String>,
    pub review_after: Option<String>,
    pub supersedes: Vec<String>,
    pub superseded_by: Option<String>,
    pub sources: Vec<MemorySource>,
    pub score: u32,
    pub matched_fields: Vec<String>,
    pub snippet: Option<String>,
}

impl MemoryRecallResult {
    pub(crate) fn from_record(
        record: MemoryRecord,
        score: u32,
        matched_fields: Vec<String>,
        snippet: Option<String>,
    ) -> Self {
        Self {
            bundle_id: record.bundle_id,
            memory_id: record.memory_id,
            concept_id: record.concept_id,
            source_path: record.source_path,
            kind: record.kind,
            status: record.status,
            title: record.title,
            description: record.description,
            body_markdown: record.body_markdown,
            owner: record.owner,
            created_at: record.created_at,
            last_verified_at: record.last_verified_at,
            verified_by: record.verified_by,
            review_after: record.review_after,
            supersedes: record.supersedes,
            superseded_by: record.superseded_by,
            sources: record.sources,
            score,
            matched_fields,
            snippet,
        }
    }
}

pub(crate) fn validate_record_request(request: &RecordMemoryRequest) -> Result<(), String> {
    require_non_empty("bundle_id", &request.bundle_id)?;
    validate_memory_id(&request.memory_id)?;
    require_non_empty("title", &request.title)?;
    require_non_empty("body_markdown", &request.body_markdown)?;
    require_non_empty("owner", &request.owner)?;
    require_non_empty("created_at", &request.created_at)?;
    if request.sources.is_empty() {
        return Err("memory sources must not be empty".into());
    }
    for source in &request.sources {
        require_non_empty("source.kind", &source.kind)?;
        require_non_empty("source.reference", &source.reference)?;
    }
    for memory_id in &request.supersedes {
        validate_memory_id(memory_id)?;
        if memory_id == &request.memory_id {
            return Err("memory cannot supersede itself".into());
        }
    }
    Ok(())
}

pub(crate) fn metadata_for_record(request: &RecordMemoryRequest) -> AgentMemoryMetadata {
    AgentMemoryMetadata {
        version: AGENT_MEMORY_VERSION,
        kind: request.kind,
        scope: REPOSITORY_MEMORY_SCOPE.into(),
        status: MemoryStatus::Candidate,
        owner: request.owner.clone(),
        created_at: request.created_at.clone(),
        last_verified_at: None,
        verified_by: None,
        review_after: request.review_after.clone(),
        supersedes: request.supersedes.clone(),
        superseded_by: None,
        sources: request.sources.clone(),
        history: Vec::new(),
    }
}

pub(crate) fn metadata_to_yaml(metadata: &AgentMemoryMetadata) -> Result<YamlValue, String> {
    yaml_serde::to_value(metadata)
        .map_err(|error| format!("memory metadata could not be serialized: {error}"))
}

pub(crate) fn metadata_from_concept(concept: &Concept) -> Result<AgentMemoryMetadata, String> {
    if concept.concept_type != AGENT_MEMORY_TYPE {
        return Err("concept is not agent memory".into());
    }
    let value = concept
        .extensions
        .get(AGENT_MEMORY_EXTENSION)
        .ok_or_else(|| "agent memory metadata is missing".to_string())?;
    let metadata = metadata_from_value(value)?;
    let memory_id = memory_id_from_concept(concept, metadata.kind)?;
    validate_memory_id(&memory_id)?;
    Ok(metadata)
}

pub(crate) fn metadata_from_value(
    value: &serde_json::Value,
) -> Result<AgentMemoryMetadata, String> {
    let metadata = serde_json::from_value::<AgentMemoryMetadata>(value.clone())
        .map_err(|_| "agent memory metadata is malformed".to_string())?;
    validate_metadata(&metadata)?;
    Ok(metadata)
}

pub(crate) fn record_from_concept(concept: &Concept) -> Result<MemoryRecord, String> {
    let metadata = metadata_from_concept(concept)?;
    record_from_metadata(concept, metadata)
}

pub(crate) fn record_from_metadata(
    concept: &Concept,
    metadata: AgentMemoryMetadata,
) -> Result<MemoryRecord, String> {
    let memory_id = memory_id_from_concept(concept, metadata.kind)?;
    Ok(MemoryRecord {
        bundle_id: concept.bundle_id.clone(),
        memory_id,
        concept_id: concept.id.clone(),
        source_path: concept.source_path.clone(),
        kind: metadata.kind,
        status: metadata.status,
        title: concept
            .title
            .clone()
            .ok_or_else(|| "agent memory title is missing".to_string())?,
        description: concept.description.clone(),
        body_markdown: concept.body_markdown.clone(),
        owner: metadata.owner,
        created_at: metadata.created_at,
        last_verified_at: metadata.last_verified_at,
        verified_by: metadata.verified_by,
        review_after: metadata.review_after,
        supersedes: metadata.supersedes,
        superseded_by: metadata.superseded_by,
        sources: metadata.sources,
        history: metadata.history,
    })
}

pub(crate) fn record_from_request(
    request: &RecordMemoryRequest,
    source_path: String,
) -> MemoryRecord {
    let metadata = metadata_for_record(request);
    MemoryRecord {
        bundle_id: request.bundle_id.clone(),
        memory_id: request.memory_id.clone(),
        concept_id: source_path.trim_end_matches(".md").to_string(),
        source_path,
        kind: request.kind,
        status: metadata.status,
        title: request.title.clone(),
        description: request.description.clone(),
        body_markdown: request.body_markdown.clone(),
        owner: metadata.owner,
        created_at: metadata.created_at,
        last_verified_at: metadata.last_verified_at,
        verified_by: metadata.verified_by,
        review_after: metadata.review_after,
        supersedes: metadata.supersedes,
        superseded_by: metadata.superseded_by,
        sources: metadata.sources,
        history: metadata.history,
    }
}

pub(crate) fn apply_transition(
    metadata: &mut AgentMemoryMetadata,
    current_memory_id: &str,
    request: &TransitionMemoryRequest,
    replacement: Option<&MemoryRecord>,
) -> Result<(), String> {
    require_non_empty("actor", &request.actor)?;
    require_non_empty("transitioned_at", &request.transitioned_at)?;
    require_non_empty("reason", &request.reason)?;
    if request.to_status != MemoryStatus::Superseded && request.replacement_id.is_some() {
        return Err("replacement_id is only valid for supersession".into());
    }

    let from = metadata.status;
    let allowed = matches!(
        (from, request.to_status),
        (MemoryStatus::Candidate, MemoryStatus::Active)
            | (MemoryStatus::Candidate, MemoryStatus::Quarantined)
            | (MemoryStatus::Active, MemoryStatus::Superseded)
            | (MemoryStatus::Active, MemoryStatus::Quarantined)
            | (MemoryStatus::Quarantined, MemoryStatus::Candidate)
    );
    if !allowed {
        return Err(format!(
            "transition from {from:?} to {:?} is not allowed",
            request.to_status
        ));
    }

    match request.to_status {
        MemoryStatus::Active => {
            metadata.last_verified_at = Some(request.transitioned_at.clone());
            metadata.verified_by = Some(request.actor.clone());
            metadata.superseded_by = None;
        }
        MemoryStatus::Candidate => {
            metadata.last_verified_at = None;
            metadata.verified_by = None;
            metadata.superseded_by = None;
        }
        MemoryStatus::Superseded => {
            let replacement_id = request
                .replacement_id
                .as_deref()
                .ok_or_else(|| "supersession requires replacement_id".to_string())?;
            let replacement =
                replacement.ok_or_else(|| "supersession replacement was not found".to_string())?;
            if replacement.status != MemoryStatus::Active {
                return Err("supersession replacement must be active".into());
            }
            if replacement.bundle_id != request.bundle_id {
                return Err("supersession replacement must be in the same bundle".into());
            }
            if replacement.memory_id != replacement_id {
                return Err("supersession replacement does not match replacement_id".into());
            }
            if !replacement
                .supersedes
                .iter()
                .any(|memory_id| memory_id == current_memory_id)
            {
                return Err("replacement must declare the memory it supersedes".into());
            }
            metadata.superseded_by = Some(replacement_id.to_string());
        }
        MemoryStatus::Quarantined => {}
    }

    metadata.status = request.to_status;
    metadata.history.push(MemoryTransition {
        from,
        to: request.to_status,
        actor: request.actor.clone(),
        at: request.transitioned_at.clone(),
        reason: request.reason.clone(),
    });
    Ok(())
}

pub(crate) fn memory_page_path(kind: MemoryKind, memory_id: &str) -> String {
    format!("memory/{}/{memory_id}.md", kind.path_segment())
}

pub(crate) fn validate_memory_id(memory_id: &str) -> Result<(), String> {
    if memory_id.is_empty()
        || !memory_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(
            "memory_id must contain only ASCII letters, digits, hyphens, or underscores".into(),
        );
    }
    Ok(())
}

fn validate_metadata(metadata: &AgentMemoryMetadata) -> Result<(), String> {
    if metadata.version != AGENT_MEMORY_VERSION {
        return Err("agent memory version is unsupported".into());
    }
    if metadata.scope != REPOSITORY_MEMORY_SCOPE {
        return Err("agent memory scope must be repository".into());
    }
    require_non_empty("owner", &metadata.owner)?;
    require_non_empty("created_at", &metadata.created_at)?;
    if metadata.sources.is_empty() {
        return Err("agent memory sources must not be empty".into());
    }
    for source in &metadata.sources {
        require_non_empty("source.kind", &source.kind)?;
        require_non_empty("source.reference", &source.reference)?;
    }
    if metadata.status == MemoryStatus::Active
        && (metadata
            .last_verified_at
            .as_deref()
            .is_none_or(str::is_empty)
            || metadata.verified_by.as_deref().is_none_or(str::is_empty))
    {
        return Err("active agent memory requires verification metadata".into());
    }
    if metadata.status == MemoryStatus::Superseded
        && metadata.superseded_by.as_deref().is_none_or(str::is_empty)
    {
        return Err("superseded agent memory requires superseded_by".into());
    }
    Ok(())
}

fn memory_id_from_concept(concept: &Concept, kind: MemoryKind) -> Result<String, String> {
    let prefix = format!("memory/{}/", kind.path_segment());
    concept
        .id
        .strip_prefix(&prefix)
        .filter(|memory_id| !memory_id.is_empty() && !memory_id.contains('/'))
        .map(str::to_string)
        .ok_or_else(|| "agent memory identity does not match its kind".to_string())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}
