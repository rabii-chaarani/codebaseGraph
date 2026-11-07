use super::{Assertion, EvidenceError, MappingSubmission};
use crate::architecture::{
    compile::{
        evidence_support as support, ArchitectureExecutionPolicy,
        ArchitectureExecutionPolicyOverride, ExecutionContext,
    },
    ArtifactRef, Digest, SourceProvenance, SourceRef,
};
use serde::{ser::SerializeMap, Serialize, Serializer};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MappingFactsDigest(Digest);
impl MappingFactsDigest {
    pub fn digest(&self) -> &Digest {
        &self.0
    }
}
fn invalid(message: &str) -> EvidenceError {
    EvidenceError::new("architecture_invalid_input", message)
}
fn limit(message: &str) -> EvidenceError {
    EvidenceError::new("resource_limit", message)
}

// These views emit fixed object keys in C22 lexical order. All leaf strings and
// records are borrowed; the only owned data is fallibly reserved sorting scratch.
#[derive(Serialize)]
struct Span {
    end: u64,
    start: u64,
}
#[derive(Serialize)]
pub(super) struct Source<'a> {
    byte_span: Span,
    commit: &'a crate::architecture::GitObjectId,
    file_digest: &'a Digest,
    node_id: &'a crate::architecture::Text,
    node_kind: &'a crate::architecture::Text,
    path: &'a crate::architecture::RelativePath,
    repository: &'a crate::architecture::Text,
    symbol: &'a crate::architecture::Text,
}
impl<'a> From<&'a SourceRef> for Source<'a> {
    fn from(v: &'a SourceRef) -> Self {
        Self {
            byte_span: Span {
                end: v.byte_span.end(),
                start: v.byte_span.start(),
            },
            commit: &v.commit,
            file_digest: &v.file_digest,
            node_id: &v.node_id,
            node_kind: &v.node_kind,
            path: &v.path,
            repository: &v.repository,
            symbol: &v.symbol,
        }
    }
}
#[derive(Serialize)]
pub(super) struct Artifact<'a> {
    commit: &'a crate::architecture::GitObjectId,
    content_digest: &'a Digest,
    path: &'a crate::architecture::RelativePath,
    repository: &'a crate::architecture::Text,
}
impl<'a> From<&'a ArtifactRef> for Artifact<'a> {
    fn from(v: &'a ArtifactRef) -> Self {
        Self {
            commit: &v.commit,
            content_digest: &v.content_digest,
            path: &v.path,
            repository: &v.repository,
        }
    }
}
struct Sources<'a>(&'a [SourceRef]);
impl Serialize for Sources<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for v in self.0 {
            seq.serialize_element(&Source::from(v))?;
        }
        seq.end()
    }
}
pub(super) struct AssertionView<'a>(pub(super) &'a Assertion);
impl Serialize for AssertionView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        match self.0 {
            Assertion::NodeExists => m.serialize_entry("kind", "node_exists")?,
            Assertion::ScopeContains { member } => {
                m.serialize_entry("kind", "scope_contains")?;
                m.serialize_entry("member", &Source::from(member))?;
            }
            Assertion::ExportedItem {
                crate_name,
                namespace,
                item_path,
            } => {
                m.serialize_entry("crate", crate_name)?;
                m.serialize_entry("item_path", item_path)?;
                m.serialize_entry("kind", "exported_item")?;
                m.serialize_entry("namespace", namespace)?;
            }
            Assertion::ReachesExportedItem {
                crate_name,
                namespace,
                item_path,
                max_hops,
                path,
            } => {
                m.serialize_entry("crate", crate_name)?;
                m.serialize_entry("item_path", item_path)?;
                m.serialize_entry("kind", "reaches_exported_item")?;
                m.serialize_entry("max_hops", max_hops)?;
                m.serialize_entry("namespace", namespace)?;
                m.serialize_entry("path", &Sources(path))?;
            }
            Assertion::LinkedContractAnchor { reference, anchor } => {
                m.serialize_entry("anchor", anchor)?;
                m.serialize_entry("kind", "linked_contract_anchor")?;
                m.serialize_entry("reference", &Artifact::from(reference))?;
            }
        }
        m.end()
    }
}
#[derive(Serialize)]
struct Citation<'a> {
    assertion: AssertionView<'a>,
    id: &'a crate::architecture::Id,
    source_ref: Source<'a>,
}
#[derive(Serialize)]
struct Resp<'a> {
    explanation: &'a crate::architecture::Text,
    responsibility_id: &'a crate::architecture::Id,
}
#[derive(Serialize)]
struct Competitor<'a> {
    component: &'a crate::architecture::Id,
    explanation: &'a crate::architecture::Text,
}
#[derive(Serialize)]
struct Rationale<'a> {
    competing_components: Vec<Competitor<'a>>,
    responsibilities: Vec<Resp<'a>>,
}
#[derive(Serialize)]
struct Inference<'a> {
    agent_id: &'a crate::architecture::Text,
    input_digest: &'a Digest,
    model_id: &'a crate::architecture::Text,
    result_digest: &'a Digest,
    rule_version: &'a crate::architecture::Text,
}
#[derive(Serialize)]
struct Lineage<'a> {
    basis: &'a super::LineageBasis,
    current: Source<'a>,
    evidence_refs: Vec<Artifact<'a>>,
    previous: Source<'a>,
    rule_version: &'a crate::architecture::Text,
}
#[derive(Serialize)]
struct Previous<'a> {
    component: &'a crate::architecture::Id,
    evidence_ref: Artifact<'a>,
    lineage: Option<Lineage<'a>>,
    mapping_facts_digest: &'a Digest,
    source_ref: Source<'a>,
}
#[derive(Serialize)]
struct Mapping<'a> {
    change_kind: &'a super::ChangeKind,
    discriminator: &'a crate::architecture::Text,
    inference_provenance: Inference<'a>,
    previous_mapping: Option<Previous<'a>>,
    proposed_component: &'a crate::architecture::Id,
    rationale: Rationale<'a>,
    responsibility_ids: Vec<&'a crate::architecture::Id>,
    source_ref: Source<'a>,
    structural_citations: Vec<Citation<'a>>,
}
#[derive(Serialize)]
pub(super) struct Provenance<'a> {
    build_context_digest: &'a Digest,
    evaluated_commit: &'a crate::architecture::GitObjectId,
    evaluated_tree: &'a crate::architecture::GitObjectId,
    extractor_digest: &'a Digest,
    repository: &'a crate::architecture::Text,
}
impl<'a> From<&'a SourceProvenance> for Provenance<'a> {
    fn from(v: &'a SourceProvenance) -> Self {
        Self {
            build_context_digest: &v.build_context_digest,
            evaluated_commit: &v.evaluated_commit,
            evaluated_tree: &v.evaluated_tree,
            extractor_digest: &v.extractor_digest,
            repository: &v.repository,
        }
    }
}
#[derive(Serialize)]
struct Payload<'a> {
    evidence_version: &'static str,
    mappings: Vec<Mapping<'a>>,
    source: Provenance<'a>,
}
fn refs<'a, T>(
    items: &'a [T],
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<Vec<&'a T>, EvidenceError> {
    let mut v = Vec::new();
    v.try_reserve(items.len())
        .map_err(|_| limit("allocation failed"))?;
    for item in items {
        control.visit(0)?;
        v.push(item);
    }
    Ok(v)
}
fn unique<T>(
    items: &[T],
    control: &mut crate::extraction_control::ExtractionControl<'_>,
    same: impl Fn(&T, &T) -> bool,
) -> Result<(), EvidenceError> {
    for pair in items.windows(2) {
        control.visit(0)?;
        if same(&pair[0], &pair[1]) {
            return Err(invalid("duplicate set key"));
        }
    }
    Ok(())
}
fn compare(a: &MappingSubmission, b: &MappingSubmission) -> std::cmp::Ordering {
    (
        a.source_ref.repository.as_str(),
        a.source_ref.commit.as_str(),
        a.source_ref.node_kind.as_str(),
        a.source_ref.node_id.as_str(),
    )
        .cmp(&(
            b.source_ref.repository.as_str(),
            b.source_ref.commit.as_str(),
            b.source_ref.node_kind.as_str(),
            b.source_ref.node_id.as_str(),
        ))
}
fn artifact_key(a: &ArtifactRef) -> (&str, &str, &str, &str) {
    (
        a.repository.as_str(),
        a.commit.as_str(),
        a.path.as_str(),
        a.content_digest.as_str(),
    )
}

/// Hash complete claims without treating their identity as validation.
pub fn mapping_facts_digest(
    source: &SourceProvenance,
    mappings: &[MappingSubmission],
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<MappingFactsDigest, EvidenceError> {
    let (policy, run) = support::start(policy, overrides, context)?;
    let mut checkpoint = || {
        run.check().map_err(|error| {
            crate::extraction_control::ExtractionError::new(
                error.code(),
                "evidence operation interrupted",
            )
        })
    };
    let mut control = crate::extraction_control::ExtractionControl::new(
        &mut checkpoint,
        policy.max_parse_depth(),
        policy.max_traversal_steps(),
        policy.max_projected_vertices(),
        policy.max_projected_edges(),
    );
    let (digest, _) = facts(source, mappings, &policy, &run, &mut control)?;
    support::measure(&digest, policy.max_result_bytes(), &run)?;
    Ok(digest)
}

pub(super) fn facts(
    source: &SourceProvenance,
    mappings: &[MappingSubmission],
    policy: &ArchitectureExecutionPolicy,
    run: &support::Run,
    control: &mut crate::extraction_control::ExtractionControl<'_>,
) -> Result<(MappingFactsDigest, u64), EvidenceError> {
    if mappings.len() as u64 > policy.max_mapping_records() {
        return Err(limit("mapping count exceeded"));
    }
    let mut sorted = refs(mappings, control)?;
    crate::extraction_control::sort_by(&mut sorted, control, |a, b| compare(a, b))?;
    unique(&sorted, control, |a, b| compare(a, b).is_eq())?;
    let mut records = Vec::new();
    records
        .try_reserve(sorted.len())
        .map_err(|_| limit("allocation failed"))?;
    for m in sorted {
        control.visit(0)?;
        if m.structural_citations.len() as u64 > policy.max_citations_per_mapping() {
            return Err(limit("citation count exceeded"));
        }
        let mut size = 0u64;
        for explanation in m
            .rationale
            .responsibilities
            .iter()
            .map(|r| &r.explanation)
            .chain(
                m.rationale
                    .competing_components
                    .iter()
                    .map(|c| &c.explanation),
            )
        {
            control.visit(0)?;
            size = size
                .checked_add(explanation.as_str().len() as u64)
                .ok_or_else(|| limit("rationale size overflow"))?;
        }
        if size > policy.max_rationale_bytes() {
            return Err(limit("rationale bytes exceeded"));
        }
        let mut ids = refs(&m.responsibility_ids, control)?;
        crate::extraction_control::sort_by(&mut ids, control, |a, b| a.as_str().cmp(b.as_str()))?;
        unique(&ids, control, |a, b| a == b)?;
        let mut rs = refs(&m.rationale.responsibilities, control)?;
        crate::extraction_control::sort_by(&mut rs, control, |a, b| {
            a.responsibility_id
                .as_str()
                .cmp(b.responsibility_id.as_str())
        })?;
        unique(&rs, control, |a, b| {
            a.responsibility_id == b.responsibility_id
        })?;
        let mut responsibilities = Vec::new();
        responsibilities
            .try_reserve(rs.len())
            .map_err(|_| limit("allocation failed"))?;
        for r in rs {
            control.visit(0)?;
            responsibilities.push(Resp {
                explanation: &r.explanation,
                responsibility_id: &r.responsibility_id,
            });
        }
        let mut cs = refs(&m.rationale.competing_components, control)?;
        crate::extraction_control::sort_by(&mut cs, control, |a, b| {
            a.component.as_str().cmp(b.component.as_str())
        })?;
        unique(&cs, control, |a, b| a.component == b.component)?;
        let mut competitors = Vec::new();
        competitors
            .try_reserve(cs.len())
            .map_err(|_| limit("allocation failed"))?;
        for c in cs {
            control.visit(0)?;
            competitors.push(Competitor {
                component: &c.component,
                explanation: &c.explanation,
            });
        }
        let mut citations = refs(&m.structural_citations, control)?;
        crate::extraction_control::sort_by(&mut citations, control, |a, b| {
            a.id.as_str().cmp(b.id.as_str())
        })?;
        unique(&citations, control, |a, b| a.id == b.id)?;
        let mut citation_views = Vec::new();
        citation_views
            .try_reserve(citations.len())
            .map_err(|_| limit("allocation failed"))?;
        for c in citations {
            control.visit(0)?;
            if let Assertion::ReachesExportedItem { path, .. } = &c.assertion {
                for _ in path {
                    control.visit(0)?;
                }
            }
            citation_views.push(Citation {
                assertion: AssertionView(&c.assertion),
                id: &c.id,
                source_ref: Source::from(&c.source_ref),
            });
        }
        let previous = if let Some(p) = &m.previous_mapping {
            let lineage = if let Some(l) = &p.lineage {
                let mut es = refs(&l.evidence_refs, control)?;
                crate::extraction_control::sort_by(&mut es, control, |a, b| {
                    artifact_key(a).cmp(&artifact_key(b))
                })?;
                unique(&es, control, |a, b| artifact_key(a) == artifact_key(b))?;
                let mut ev = Vec::new();
                ev.try_reserve(es.len())
                    .map_err(|_| limit("allocation failed"))?;
                for e in es {
                    control.visit(0)?;
                    ev.push(Artifact::from(e));
                }
                Some(Lineage {
                    basis: &l.basis,
                    current: Source::from(&l.current),
                    evidence_refs: ev,
                    previous: Source::from(&l.previous),
                    rule_version: &l.rule_version,
                })
            } else {
                None
            };
            Some(Previous {
                component: &p.component,
                evidence_ref: Artifact::from(&p.evidence_ref),
                lineage,
                mapping_facts_digest: &p.mapping_facts_digest,
                source_ref: Source::from(&p.source_ref),
            })
        } else {
            None
        };
        let i = &m.inference_provenance;
        records.push(Mapping {
            change_kind: &m.change_kind,
            discriminator: &m.discriminator,
            inference_provenance: Inference {
                agent_id: &i.agent_id,
                input_digest: &i.input_digest,
                model_id: &i.model_id,
                result_digest: &i.result_digest,
                rule_version: &i.rule_version,
            },
            previous_mapping: previous,
            proposed_component: &m.proposed_component,
            rationale: Rationale {
                competing_components: competitors,
                responsibilities,
            },
            responsibility_ids: ids,
            source_ref: Source::from(&m.source_ref),
            structural_citations: citation_views,
        });
    }
    let payload = Payload {
        evidence_version: "architecture-evidence/0.1",
        mappings: records,
        source: Provenance {
            build_context_digest: &source.build_context_digest,
            evaluated_commit: &source.evaluated_commit,
            evaluated_tree: &source.evaluated_tree,
            extractor_digest: &source.extractor_digest,
            repository: &source.repository,
        },
    };
    let (digest, bytes) =
        support::stream_hash(&payload, policy.max_evidence_bytes(), run, control)?;
    Ok((MappingFactsDigest(digest), bytes))
}

/// Built-in extraction semantics; this identifies an implementation, not a build receipt.
#[derive(Debug, Serialize)]
pub struct SourceExtractorIdentity {
    descriptor: SourceExtractorDescriptor,
    digest: Digest,
}
impl SourceExtractorIdentity {
    pub fn descriptor(&self) -> &SourceExtractorDescriptor {
        &self.descriptor
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}
#[derive(Debug, Serialize)]
pub struct SourceExtractorDescriptor {
    version: &'static str,
    language: &'static str,
    profile: crate::protocol::LanguageProfile,
    grammar_node_types_digest: Digest,
    node_identity_version: &'static str,
}
impl SourceExtractorDescriptor {
    pub fn version(&self) -> &str {
        self.version
    }
    pub fn language(&self) -> &str {
        self.language
    }
    pub fn profile(&self) -> &crate::protocol::LanguageProfile {
        &self.profile
    }
    pub fn grammar_node_types_digest(&self) -> &Digest {
        &self.grammar_node_types_digest
    }
    pub fn node_identity_version(&self) -> &str {
        self.node_identity_version
    }
}
/// Identify the linked built-in Rust extractor without implying source qualification.
pub fn source_extractor_identity(
    context: &ExecutionContext,
) -> Result<SourceExtractorIdentity, EvidenceError> {
    let run = support::metadata_run(context);
    extractor_in_run(&run)
}

pub(super) fn extractor_in_run(
    run: &support::Run,
) -> Result<SourceExtractorIdentity, EvidenceError> {
    run.check()?;
    let profile = crate::profiles::built_in_profiles()
        .into_iter()
        .find(|p| p.language == "rust")
        .ok_or_else(|| {
            EvidenceError::new(
                "architecture_internal_error",
                "built-in Rust profile missing",
            )
        })?;
    run.check()?;
    let node_types = crate::parser::grammar_node_types(&profile).ok_or_else(|| {
        EvidenceError::new(
            "architecture_internal_error",
            "built-in Rust grammar missing",
        )
    })?;
    let grammar_node_types_digest = support::raw_digest(node_types.as_bytes(), run)?;
    let descriptor = SourceExtractorDescriptor {
        version: "native-source-evidence/0.1",
        language: "rust",
        profile,
        grammar_node_types_digest,
        node_identity_version: "native-source-graph/1",
    };
    let digest = support::metadata_hash("architecture-source-extractor", &descriptor, run)?;
    Ok(SourceExtractorIdentity { descriptor, digest })
}
