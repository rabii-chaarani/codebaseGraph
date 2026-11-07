//! Untrusted evidence claims. These types do not establish source or model facts.
use super::super::{ArtifactRef, Digest, Id, SourceRef, Text};
use serde::{Deserialize, Deserializer, Serialize};

// Serde otherwise treats an absent Option field as null. These fields must be
// present on the wire, even when their value is null.
fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappingSubmission {
    pub source_ref: SourceRef,
    pub proposed_component: Id,
    pub responsibility_ids: Vec<Id>,
    pub structural_citations: Vec<StructuralCitation>,
    pub rationale: Rationale,
    pub inference_provenance: InferenceProvenance,
    #[serde(deserialize_with = "required_option")]
    pub previous_mapping: Option<PreviousMapping>,
    pub change_kind: ChangeKind,
    pub discriminator: Text,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Initial,
    Retained,
    Correction,
    Transfer,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rationale {
    pub responsibilities: Vec<ResponsibilityExplanation>,
    pub competing_components: Vec<ComponentExplanation>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsibilityExplanation {
    pub responsibility_id: Id,
    pub explanation: Text,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentExplanation {
    pub component: Id,
    pub explanation: Text,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferenceProvenance {
    pub agent_id: Text,
    pub model_id: Text,
    pub input_digest: Digest,
    pub rule_version: Text,
    pub result_digest: Digest,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuralCitation {
    pub id: Id,
    pub source_ref: SourceRef,
    pub assertion: Assertion,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Assertion {
    NodeExists,
    ScopeContains {
        member: SourceRef,
    },
    ExportedItem {
        #[serde(rename = "crate")]
        crate_name: Text,
        namespace: Text,
        item_path: Text,
    },
    ReachesExportedItem {
        #[serde(rename = "crate")]
        crate_name: Text,
        namespace: Text,
        item_path: Text,
        #[serde(deserialize_with = "required_option")]
        max_hops: Option<std::num::NonZeroU64>,
        path: Vec<SourceRef>,
    },
    LinkedContractAnchor {
        reference: ArtifactRef,
        anchor: Text,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviousMapping {
    pub source_ref: SourceRef,
    pub component: Id,
    pub mapping_facts_digest: Digest,
    pub evidence_ref: ArtifactRef,
    #[serde(deserialize_with = "required_option")]
    pub lineage: Option<LineageRecord>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageRecord {
    pub previous: SourceRef,
    pub current: SourceRef,
    pub rule_version: Text,
    pub basis: LineageBasis,
    pub evidence_refs: Vec<ArtifactRef>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageBasis {
    UnchangedQualifiedIdentity,
    UniqueStructuralMove,
}
