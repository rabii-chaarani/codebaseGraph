//! Transport-neutral, bounded compilation of an immutable architecture closure.
mod constraints;
pub mod identity;
mod output;
mod policy;
mod scenarios;
mod validate;

mod source;

mod yaml;

use super::RelativePath;
pub use source::{ClosureFile, ModelInput, PinnedProvenance, RepositoryBinding};

pub use policy::{
    ArchitectureExecutionPolicy, ArchitectureExecutionPolicyOverride, ExecutionContext,
};

/// A position in an authored UTF-8 file, measured in bytes.
#[derive(Clone, Debug)]
pub struct SourceLocation {
    path: RelativePath,
    start: usize,
    end: usize,
    line: usize,
    column: usize,
}
impl SourceLocation {
    pub fn path(&self) -> &RelativePath {
        &self.path
    }
    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }
    pub fn line(&self) -> usize {
        self.line
    }
    pub fn column(&self) -> usize {
        self.column
    }
}
/// Diagnostic emitted during compilation.
#[derive(Clone, Debug)]
pub struct CompileDiagnostic {
    code: &'static str,
    reason: String,
    subject: Option<String>,
    location: Option<SourceLocation>,
}
impl CompileDiagnostic {
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }
    pub fn location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }
}
/// Failed compilation; never contains a partial successful model.
#[derive(Clone, Debug)]
pub struct CompileError {
    code: &'static str,
    diagnostics: Vec<CompileDiagnostic>,
    object_integrity_failure: bool,
}
impl CompileError {
    fn simple(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            object_integrity_failure: false,
            diagnostics: vec![CompileDiagnostic {
                code,
                reason: reason.into(),
                subject: None,
                location: None,
            }],
        }
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn diagnostics(&self) -> &[CompileDiagnostic] {
        &self.diagnostics
    }
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for CompileError {}

pub use constraints::{
    AuthorizationContextRef, BaselineRef, Budget, ConstraintDocument, EdgeRule, Metric,
    OwnershipClause, OwnershipRule, ProjectionRef, Waiver,
};
pub use scenarios::{
    ConstraintKind, ConstraintRecord, ElementKind, ElementRecord, ElementRef, Operation, Parameter,
    Predicate, Scenario,
};

/// An immutable complete compilation, without an evaluation or accepted-state identity.
pub struct CompiledModel {
    root: super::ModelRoot,
    architecture: Vec<super::ArchitectureDocument>,
    constraints: Vec<ConstraintDocument>,
    closure: Vec<ClosureFile>,
    root_index: usize,
    effective_policy: ArchitectureExecutionPolicy,
    provenance: Option<PinnedProvenance>,
    canonical_json: Vec<u8>,
}
impl CompiledModel {
    pub fn root(&self) -> &super::ModelRoot {
        &self.root
    }
    pub fn architecture(&self) -> &[super::ArchitectureDocument] {
        &self.architecture
    }
    pub fn constraints(&self) -> &[ConstraintDocument] {
        &self.constraints
    }
    pub fn closure(&self) -> &[ClosureFile] {
        &self.closure
    }
    pub fn effective_policy(&self) -> &ArchitectureExecutionPolicy {
        &self.effective_policy
    }
    pub fn provenance(&self) -> Option<&PinnedProvenance> {
        self.provenance.as_ref()
    }
    /// Canonical complete logical result; the derived cache is excluded from itself.
    pub fn canonical_json(&self) -> &[u8] {
        &self.canonical_json
    }
}

/// Compile one explicitly captured immutable model closure.
pub fn compile(
    input: ModelInput,
    repository: Option<&RepositoryBinding>,
    policy: &ArchitectureExecutionPolicy,
    overrides: Option<&ArchitectureExecutionPolicyOverride>,
    context: &ExecutionContext,
) -> Result<CompiledModel, CompileError> {
    let started = std::time::Instant::now();
    let effective_policy = policy.effective(overrides)?;
    let run = context.start_at(&effective_policy, started)?;
    run.check()?;
    let mut reader = source::Reader::new(input, repository, &effective_policy, &run)?;
    let root_path = reader.root_path().clone();
    let root_file = reader.root()?;
    let root_value = yaml::parse(
        root_file.bytes(),
        &root_path,
        effective_policy
            .max_file_bytes
            .min(effective_policy.max_yaml_bytes),
        effective_policy.max_parse_depth,
        &run,
    )?;
    let root = validate::decode_root(&root_value, &run)?;
    let mut paths = Vec::new();
    let count = root
        .architecture
        .len()
        .checked_add(root.constraints.len())
        .ok_or_else(|| CompileError::simple("resource_limit", "closure count overflow"))?;
    paths
        .try_reserve(count)
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    paths.extend(root.architecture.iter().cloned());
    paths.extend(root.constraints.iter().cloned());
    let directory = root_path
        .as_str()
        .rsplit_once('/')
        .map(|(d, _)| d)
        .unwrap_or("");
    let (closure, provenance) = match reader.finish(&paths) {
        Ok(result) => result,
        Err(mut error) => {
            if let yaml::Kind::Mapping(entries) = &root_value.kind {
                for (key, values) in entries {
                    run.check()?;
                    if !matches!(&key.kind, yaml::Kind::String(name) if name == "architecture" || name == "constraints")
                    {
                        continue;
                    }
                    if let yaml::Kind::Sequence(items) = &values.kind {
                        for item in items {
                            run.check()?;
                            if let yaml::Kind::String(path) = &item.kind {
                                let resolved = if directory.is_empty() {
                                    path.clone()
                                } else {
                                    format!("{directory}/{path}")
                                };
                                for diagnostic in &mut error.diagnostics {
                                    if diagnostic.subject.as_deref() == Some(resolved.as_str()) {
                                        diagnostic.location = Some(item.location.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return Err(error);
        }
    };
    let root_index = closure
        .binary_search_by(|f| f.path().as_str().cmp(root_path.as_str()))
        .map_err(|_| {
            CompileError::simple(
                "architecture_internal_error",
                "root missing from captured closure",
            )
        })?;
    let find_file = |path: &RelativePath| -> Result<&ClosureFile, CompileError> {
        let resolved = if directory.is_empty() {
            path.as_str().to_owned()
        } else {
            format!("{directory}/{}", path.as_str())
        };
        closure
            .binary_search_by(|f| f.path().as_str().cmp(&resolved))
            .ok()
            .map(|i| &closure[i])
            .ok_or_else(|| {
                CompileError::simple("architecture_input_unavailable", "closure file unavailable")
            })
    };
    let mut locations = std::collections::HashMap::new();
    let mut entities = 0;
    let mut architecture = Vec::new();
    architecture
        .try_reserve(root.architecture.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    for path in &root.architecture {
        run.check()?;
        let file = find_file(path)?;
        let value = yaml::parse(
            file.bytes(),
            file.path(),
            effective_policy.max_file_bytes,
            effective_policy.max_parse_depth,
            &run,
        )?;
        let doc = validate::decode_architecture(
            &value,
            &mut entities,
            effective_policy.max_authored_entities,
            &run,
        )?;
        validate::index_record_locations(&value, &mut locations, &run)?;
        architecture.push(doc);
    }
    let mut constraints = Vec::new();
    constraints
        .try_reserve(root.constraints.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    let mut counts = scenarios::decode::Counts {
        entities,
        scenarios: 0,
        operations: 0,
        expanded_bytes: 0,
        expanded_limit: effective_policy.max_result_bytes,
    };
    let mut constraint_values = Vec::new();
    constraint_values
        .try_reserve(root.constraints.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    for path in &root.constraints {
        run.check()?;
        let file = find_file(path)?;
        let value = yaml::parse(
            file.bytes(),
            file.path(),
            effective_policy.max_file_bytes,
            effective_policy.max_parse_depth,
            &run,
        )?;
        let (doc, _) = constraints::decode::document(
            &value,
            &mut counts.entities,
            effective_policy.max_authored_entities,
            &run,
        )?;
        validate::index_record_locations(&value, &mut locations, &run)?;
        constraints.push(doc);
        constraint_values.push(value);
    }
    let mut result = CompiledModel {
        root,
        architecture,
        constraints,
        closure,
        root_index,
        effective_policy,
        provenance,
        canonical_json: Vec::new(),
    };
    // Empty scenario lists preserve their array delimiters in this base. Each
    // scenario preflight adds its complete canonical bytes and separators.
    counts.expanded_bytes = u64::try_from(output::encode(&result, &run)?.len())
        .map_err(|_| CompileError::simple("resource_limit", "result size overflow"))?;
    for (doc, value) in result.constraints.iter_mut().zip(&constraint_values) {
        run.check()?;
        let yaml::Kind::Mapping(entries) = &value.kind else {
            unreachable!("decoded constraint document")
        };
        let scenarios = entries
            .iter()
            .find_map(|(key, value)| {
                if matches!(&key.kind, yaml::Kind::String(name) if name == "scenarios") {
                    Some(value)
                } else {
                    None
                }
            })
            .expect("decoded document has scenarios");
        let yaml::Kind::Sequence(pending) = &scenarios.kind else {
            unreachable!("decoded scenario list")
        };
        scenarios::decode::separators(&mut counts, pending.len().saturating_sub(1), &run)?;
        doc.scenarios = scenarios::decode::scenarios(
            pending,
            &mut counts,
            &mut locations,
            &result.effective_policy,
            &run,
        )?;
    }
    validate::validate_base(
        &mut result.architecture,
        &mut result.constraints,
        &locations,
        &result.effective_policy,
        &run,
    )?;
    run.check()?;
    result.canonical_json = output::encode(&result, &run)?;
    if counts.expanded_bytes != result.canonical_json.len() as u64 {
        return Err(CompileError::simple(
            "architecture_internal_error",
            "canonical preflight size mismatch",
        ));
    }
    Ok(result)
}
