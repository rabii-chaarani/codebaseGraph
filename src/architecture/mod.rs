//! Transport-neutral architecture contract. Primitive and reference constructors
//! do not qualify linked artifacts or evidence. The compiler verifies its pinned
//! model source closure; it does not install a schema or establish artifact trust.
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = include_str!("../../assets/architecture_schema.json");

fn checked<'de, D, F>(deserializer: D, valid: F, expected: &str) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
    F: FnOnce(&str) -> bool,
{
    let value = String::deserialize(deserializer)?;
    if valid(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(format!("invalid {expected}")))
    }
}
fn id_valid(s: &str) -> bool {
    let mut bytes = s.bytes();
    if !bytes.next().is_some_and(|b| b.is_ascii_lowercase()) {
        return false;
    }
    let mut separator = false;
    for b in bytes {
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            separator = false;
        } else if matches!(b, b'-' | b'_' | b'.') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}
fn path_valid(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('\\')
        && !s.contains('\0')
        && !matches!(s.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
        && s.split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
macro_rules! string_type {
    ($name:ident, $predicate:expr, $label:literal) => {
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if ($predicate)(&value) {
                    Ok(Self(value))
                } else {
                    Err(concat!("invalid ", $label).into())
                }
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                checked(d, $predicate, $label).map(Self)
            }
        }
    };
}
string_type!(Id, id_valid, "ID");
string_type!(Text, |s: &str| !s.trim().is_empty(), "text");
string_type!(RelativePath, path_valid, "relative path");
string_type!(
    Digest,
    |s: &str| s
        .strip_prefix("sha256:")
        .is_some_and(|v| v.len() == 64 && hex(v)),
    "digest"
);
string_type!(
    GitObjectId,
    |s: &str| (s.len() == 40 || s.len() == 64) && hex(s),
    "Git object ID"
);
string_type!(SchemaVersion, |s: &str| s == "0.1", "schema version");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitRevision {
    pub repository: Text,
    pub commit: GitObjectId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub repository: Text,
    pub commit: GitObjectId,
    pub path: RelativePath,
    pub content_digest: Digest,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ByteSpan {
    start: u64,
    end: u64,
}
impl<'de> Deserialize<'de> for ByteSpan {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            start: u64,
            end: u64,
        }
        let raw = Raw::deserialize(d)?;
        Self::new(raw.start, raw.end).map_err(serde::de::Error::custom)
    }
}
impl ByteSpan {
    pub fn start(&self) -> u64 {
        self.start
    }
    pub fn end(&self) -> u64 {
        self.end
    }
    pub fn new(start: u64, end: u64) -> Result<Self, &'static str> {
        if start < end {
            Ok(Self { start, end })
        } else {
            Err("invalid byte span")
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub repository: Text,
    pub commit: GitObjectId,
    pub path: RelativePath,
    pub node_kind: Text,
    pub node_id: Text,
    pub symbol: Text,
    pub byte_span: ByteSpan,
    pub file_digest: Digest,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProvenance {
    pub repository: Text,
    pub evaluated_commit: GitObjectId,
    pub evaluated_tree: GitObjectId,
    pub extractor_digest: Digest,
    pub build_context_digest: Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    AcceptedIntent,
    CandidateIntent,
    ObservedProjection,
    PredictedProjection,
    Evaluation,
}
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowIdentity {
    pub model_id: Id,
    pub snapshot_id: Id,
    pub logical_id: Id,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowMetadata {
    pub model_id: Id,
    pub snapshot_id: Id,
    pub state_kind: StateKind,
    pub schema_version: SchemaVersion,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Responsibility {
    pub id: Id,
    pub statement: Text,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct System {
    pub id: Id,
    pub name: Text,
    pub responsibilities: Vec<Responsibility>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerKind {
    Application,
    DataStore,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub id: Id,
    pub name: Text,
    pub responsibilities: Vec<Responsibility>,
    pub system: Id,
    pub kind: ContainerKind,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: Id,
    pub name: Text,
    pub responsibilities: Vec<Responsibility>,
    pub container: Id,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Container,
    Component,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub id: Id,
    pub level: Level,
    pub from: Id,
    pub to: Id,
    pub description: Text,
    pub contract_refs: Vec<ArtifactRef>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitectureDocument {
    pub schema_version: SchemaVersion,
    pub systems: Vec<System>,
    pub containers: Vec<Container>,
    pub components: Vec<Component>,
    pub relationships: Vec<Relationship>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRoot {
    pub schema_version: SchemaVersion,
    pub model_id: Id,
    pub architecture: Vec<RelativePath>,
    pub constraints: Vec<RelativePath>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub subject: String,
    pub code: &'static str,
}

/// Validate the complete explicitly assembled architecture document set. The caller
/// must also validate IDs in constraint/scenario documents when assembling those.
pub fn validate(documents: &[ArchitectureDocument]) -> Vec<Diagnostic> {
    match validate_with_checkpoint(documents, || Ok::<(), std::convert::Infallible>(())) {
        Ok(diagnostics) => diagnostics,
        Err(never) => match never {},
    }
}

fn validate_with_checkpoint<E>(
    documents: &[ArchitectureDocument],
    mut checkpoint: impl FnMut() -> Result<(), E>,
) -> Result<Vec<Diagnostic>, E> {
    checkpoint()?;
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    let mut systems = HashSet::new();
    let mut containers = HashSet::new();
    let mut components = HashSet::new();
    let mut add = |id: &Id| {
        if !ids.insert(id.as_str().to_owned()) {
            out.push(Diagnostic {
                subject: id.as_str().into(),
                code: "duplicate_id",
            });
        }
    };
    for d in documents {
        checkpoint()?;
        for s in &d.systems {
            checkpoint()?;
            add(&s.id);
            systems.insert(s.id.as_str());
            for r in &s.responsibilities {
                checkpoint()?;
                add(&r.id);
            }
        }
        for c in &d.containers {
            checkpoint()?;
            add(&c.id);
            containers.insert(c.id.as_str());
            for r in &c.responsibilities {
                checkpoint()?;
                add(&r.id);
            }
        }
        for c in &d.components {
            checkpoint()?;
            add(&c.id);
            components.insert(c.id.as_str());
            for r in &c.responsibilities {
                checkpoint()?;
                add(&r.id);
            }
        }
        for r in &d.relationships {
            checkpoint()?;
            add(&r.id);
        }
    }
    let mut counts = HashMap::new();
    for d in documents {
        checkpoint()?;
        for s in &d.systems {
            checkpoint()?;
            if s.responsibilities.is_empty() {
                out.push(Diagnostic {
                    subject: s.id.as_str().into(),
                    code: "missing_responsibility",
                });
            }
        }
        for c in &d.containers {
            checkpoint()?;
            if c.responsibilities.is_empty() {
                out.push(Diagnostic {
                    subject: c.id.as_str().into(),
                    code: "missing_responsibility",
                });
            }
            if !systems.contains(c.system.as_str()) {
                out.push(Diagnostic {
                    subject: c.id.as_str().into(),
                    code: "invalid_parent",
                });
            }
            *counts.entry(c.id.as_str()).or_insert(0usize) += 1;
        }
        for c in &d.components {
            checkpoint()?;
            if c.responsibilities.is_empty() {
                out.push(Diagnostic {
                    subject: c.id.as_str().into(),
                    code: "missing_responsibility",
                });
            }
            if !containers.contains(c.container.as_str()) {
                out.push(Diagnostic {
                    subject: c.id.as_str().into(),
                    code: "invalid_parent",
                });
            }
            *counts.entry(c.id.as_str()).or_insert(0usize) += 1;
        }
        for r in &d.relationships {
            checkpoint()?;
            let allowed = match r.level {
                Level::Container => &containers,
                Level::Component => &components,
            };
            if !allowed.contains(r.from.as_str()) || !allowed.contains(r.to.as_str()) {
                out.push(Diagnostic {
                    subject: r.id.as_str().into(),
                    code: "invalid_endpoint",
                });
            }
        }
    }
    for (id, n) in counts {
        checkpoint()?;
        if n > 1 {
            out.push(Diagnostic {
                subject: id.into(),
                code: "contradictory_parentage",
            });
        }
    }
    checkpoint()?;
    out.sort_by(|a, b| (&a.subject, a.code).cmp(&(&b.subject, b.code)));
    checkpoint()?;
    Ok(out)
}

/// Strict, bounded architecture model compilation.
pub mod compile;
