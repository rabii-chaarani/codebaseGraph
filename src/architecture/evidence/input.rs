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

// A successful result owns the exact submitted claims; copy candidate-scaled
// strings and arrays under the same execution guard used by validation.
pub(super) fn copy_submission(
    value: &MappingSubmission,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<MappingSubmission, super::EvidenceError> {
    use super::EvidenceError;
    use crate::architecture::{
        ArtifactRef, ByteSpan, Digest, GitObjectId, Id, RelativePath, SourceRef, Text,
    };
    use crate::extraction_control::{copy_text, ExtractionControl};
    fn id(v: &Id, c: &mut ExtractionControl<'_>) -> Result<Id, EvidenceError> {
        Id::new(copy_text(v.as_str(), c)?)
            .map_err(|e| EvidenceError::new("architecture_internal_error", e))
    }
    fn text(v: &Text, c: &mut ExtractionControl<'_>) -> Result<Text, EvidenceError> {
        Text::new(copy_text(v.as_str(), c)?)
            .map_err(|e| EvidenceError::new("architecture_internal_error", e))
    }
    fn digest(v: &Digest, c: &mut ExtractionControl<'_>) -> Result<Digest, EvidenceError> {
        Digest::new(copy_text(v.as_str(), c)?)
            .map_err(|e| EvidenceError::new("architecture_internal_error", e))
    }
    fn revision(
        v: &GitObjectId,
        c: &mut ExtractionControl<'_>,
    ) -> Result<GitObjectId, EvidenceError> {
        GitObjectId::new(copy_text(v.as_str(), c)?)
            .map_err(|e| EvidenceError::new("architecture_internal_error", e))
    }
    fn path(
        v: &RelativePath,
        c: &mut ExtractionControl<'_>,
    ) -> Result<RelativePath, EvidenceError> {
        RelativePath::new(copy_text(v.as_str(), c)?)
            .map_err(|e| EvidenceError::new("architecture_internal_error", e))
    }
    fn source(v: &SourceRef, c: &mut ExtractionControl<'_>) -> Result<SourceRef, EvidenceError> {
        Ok(SourceRef {
            repository: text(&v.repository, c)?,
            commit: revision(&v.commit, c)?,
            path: path(&v.path, c)?,
            node_kind: text(&v.node_kind, c)?,
            node_id: text(&v.node_id, c)?,
            symbol: text(&v.symbol, c)?,
            byte_span: ByteSpan::new(v.byte_span.start(), v.byte_span.end())
                .map_err(|e| EvidenceError::new("architecture_internal_error", e))?,
            file_digest: digest(&v.file_digest, c)?,
        })
    }
    fn artifact(
        v: &ArtifactRef,
        c: &mut ExtractionControl<'_>,
    ) -> Result<ArtifactRef, EvidenceError> {
        Ok(ArtifactRef {
            repository: text(&v.repository, c)?,
            commit: revision(&v.commit, c)?,
            path: path(&v.path, c)?,
            content_digest: digest(&v.content_digest, c)?,
        })
    }
    fn list<T, U>(
        v: &[T],
        c: &mut ExtractionControl<'_>,
        mut f: impl FnMut(&T, &mut ExtractionControl<'_>) -> Result<U, EvidenceError>,
    ) -> Result<Vec<U>, EvidenceError> {
        let mut out = Vec::new();
        out.try_reserve(v.len())
            .map_err(|_| EvidenceError::new("resource_limit", "allocation failed"))?;
        for item in v {
            c.visit(0)?;
            out.push(f(item, c)?);
        }
        Ok(out)
    }
    fn assertion(v: &Assertion, c: &mut ExtractionControl<'_>) -> Result<Assertion, EvidenceError> {
        Ok(match v {
            Assertion::NodeExists => Assertion::NodeExists,
            Assertion::ScopeContains { member } => Assertion::ScopeContains {
                member: source(member, c)?,
            },
            Assertion::ExportedItem {
                crate_name,
                namespace,
                item_path,
            } => Assertion::ExportedItem {
                crate_name: text(crate_name, c)?,
                namespace: text(namespace, c)?,
                item_path: text(item_path, c)?,
            },
            Assertion::ReachesExportedItem {
                crate_name,
                namespace,
                item_path,
                max_hops,
                path,
            } => Assertion::ReachesExportedItem {
                crate_name: text(crate_name, c)?,
                namespace: text(namespace, c)?,
                item_path: text(item_path, c)?,
                max_hops: *max_hops,
                path: list(path, c, source)?,
            },
            Assertion::LinkedContractAnchor { reference, anchor } => {
                Assertion::LinkedContractAnchor {
                    reference: artifact(reference, c)?,
                    anchor: text(anchor, c)?,
                }
            }
        })
    }
    let previous_mapping = if let Some(p) = &value.previous_mapping {
        let lineage = if let Some(l) = &p.lineage {
            Some(LineageRecord {
                previous: source(&l.previous, control)?,
                current: source(&l.current, control)?,
                rule_version: text(&l.rule_version, control)?,
                basis: l.basis.clone(),
                evidence_refs: list(&l.evidence_refs, control, artifact)?,
            })
        } else {
            None
        };
        Some(PreviousMapping {
            source_ref: source(&p.source_ref, control)?,
            component: id(&p.component, control)?,
            mapping_facts_digest: digest(&p.mapping_facts_digest, control)?,
            evidence_ref: artifact(&p.evidence_ref, control)?,
            lineage,
        })
    } else {
        None
    };
    let mut result = MappingSubmission {
        source_ref: source(&value.source_ref, control)?,
        proposed_component: id(&value.proposed_component, control)?,
        responsibility_ids: list(&value.responsibility_ids, control, id)?,
        structural_citations: list(&value.structural_citations, control, |v, c| {
            Ok(StructuralCitation {
                id: id(&v.id, c)?,
                source_ref: source(&v.source_ref, c)?,
                assertion: assertion(&v.assertion, c)?,
            })
        })?,
        rationale: Rationale {
            responsibilities: list(&value.rationale.responsibilities, control, |v, c| {
                Ok(ResponsibilityExplanation {
                    responsibility_id: id(&v.responsibility_id, c)?,
                    explanation: text(&v.explanation, c)?,
                })
            })?,
            competing_components: list(&value.rationale.competing_components, control, |v, c| {
                Ok(ComponentExplanation {
                    component: id(&v.component, c)?,
                    explanation: text(&v.explanation, c)?,
                })
            })?,
        },
        inference_provenance: InferenceProvenance {
            agent_id: text(&value.inference_provenance.agent_id, control)?,
            model_id: text(&value.inference_provenance.model_id, control)?,
            input_digest: digest(&value.inference_provenance.input_digest, control)?,
            rule_version: text(&value.inference_provenance.rule_version, control)?,
            result_digest: digest(&value.inference_provenance.result_digest, control)?,
        },
        previous_mapping,
        change_kind: value.change_kind.clone(),
        discriminator: text(&value.discriminator, control)?,
    };
    crate::extraction_control::sort_by(&mut result.responsibility_ids, control, |a, b| {
        a.as_str().cmp(b.as_str())
    })?;
    crate::extraction_control::sort_by(&mut result.rationale.responsibilities, control, |a, b| {
        a.responsibility_id
            .as_str()
            .cmp(b.responsibility_id.as_str())
    })?;
    crate::extraction_control::sort_by(
        &mut result.rationale.competing_components,
        control,
        |a, b| a.component.as_str().cmp(b.component.as_str()),
    )?;
    crate::extraction_control::sort_by(&mut result.structural_citations, control, |a, b| {
        a.id.as_str().cmp(b.id.as_str())
    })?;
    if let Some(l) = result
        .previous_mapping
        .as_mut()
        .and_then(|p| p.lineage.as_mut())
    {
        crate::extraction_control::sort_by(&mut l.evidence_refs, control, |a, b| {
            (
                a.repository.as_str(),
                a.commit.as_str(),
                a.path.as_str(),
                a.content_digest.as_str(),
            )
                .cmp(&(
                    b.repository.as_str(),
                    b.commit.as_str(),
                    b.path.as_str(),
                    b.content_digest.as_str(),
                ))
        })?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_copy_is_canonical_and_interruptible() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let reference = serde_json::json!({"repository":"repo","commit":"1".repeat(40),"path":"src/lib.rs","node_kind":"Function","node_id":"node","symbol":"name","byte_span":{"start":0,"end":1},"file_digest":digest});
        let value = serde_json::json!({"source_ref":reference,"proposed_component":"component","responsibility_ids":["z","a"],"structural_citations":[],"rationale":{"responsibilities":[],"competing_components":[]},"inference_provenance":{"agent_id":"agent","model_id":"model","input_digest":digest,"rule_version":"responsibility-evidence/0.1","result_digest":digest},"previous_mapping":null,"change_kind":"initial","discriminator":"responsibility-evidence/0.1"});
        let input: MappingSubmission = serde_json::from_value(value).unwrap();
        let mut check = || Ok(());
        let mut control =
            crate::extraction_control::ExtractionControl::new(&mut check, 100, 100_000, 100, 100);
        let copy = copy_submission(&input, &mut control).unwrap();
        assert_eq!(
            copy.responsibility_ids
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert_eq!(copy.source_ref, input.source_ref);
        let mut calls = 0;
        let mut check = || {
            calls += 1;
            if calls > 10 {
                Err(crate::extraction_control::ExtractionError::new(
                    "cancelled",
                    "cancelled",
                ))
            } else {
                Ok(())
            }
        };
        let mut control =
            crate::extraction_control::ExtractionControl::new(&mut check, 100, 100_000, 100, 100);
        assert_eq!(
            copy_submission(&input, &mut control).unwrap_err().code(),
            "cancelled"
        );
    }
}
