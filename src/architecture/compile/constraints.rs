//! Typed constraint vocabulary. Validation of cross-document references is separate.
use super::super::{ArtifactRef, Digest, Id, Level, SchemaVersion, Text};
use super::scenarios::Scenario;
#[cfg(test)]
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct ProjectionRef {
    pub(super) version: String,
    pub(super) domain_ref: Digest,
}
#[derive(Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BaselineRef {
    #[non_exhaustive]
    Evaluation { id: Digest },
    #[non_exhaustive]
    Input { id: Digest },
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct AuthorizationContextRef {
    pub(super) reference: ArtifactRef,
    pub(super) context_id: Id,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnershipClause {
    #[non_exhaustive]
    ExportedItem {
        #[serde(rename = "crate")]
        crate_name: Text,
        namespace: Text,
        item_path: Text,
    },
    #[non_exhaustive]
    ReachesExportedItem {
        #[serde(rename = "crate")]
        crate_name: Text,
        namespace: Text,
        item_path: Text,
        #[serde(skip_serializing_if = "Option::is_none")]
        max_hops: Option<u64>,
    },
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct OwnershipRule {
    pub(super) id: Id,
    pub(super) component: Id,
    pub(super) responsibility_ids: Vec<Id>,
    pub(super) all_of: Vec<OwnershipClause>,
    pub(super) none_of: Vec<OwnershipClause>,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct EdgeRule {
    pub(super) id: Id,
    pub(super) level: Level,
    pub(super) from: Vec<Id>,
    pub(super) to: Vec<Id>,
    pub(super) projection: ProjectionRef,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    CyclicNodes,
    MaxDirectFanout,
    MaxTransitiveReach,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub(super) id: Id,
    pub(super) level: Level,
    pub(super) origins: Vec<Id>,
    pub(super) projection: ProjectionRef,
    pub(super) metric: Metric,
    pub(super) absolute_max: u64,
    pub(super) max_increase: u64,
    pub(super) baseline: BaselineRef,
}
#[derive(Debug, Serialize)]
#[cfg_attr(test, derive(Clone, Deserialize))]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub(super) id: Id,
    pub(super) budget_id: Id,
    pub(super) baseline: BaselineRef,
    pub(super) original_limit: u64,
    pub(super) ceiling: u64,
    pub(super) expires_at: String,
    pub(super) approval_ref: AuthorizationContextRef,
}
#[cfg_attr(test, derive(Clone, Debug, Serialize, Deserialize))]
#[cfg_attr(test, serde(deny_unknown_fields))]
pub struct ConstraintDocument {
    pub(super) schema_version: SchemaVersion,
    pub(super) ownership_rules: Vec<OwnershipRule>,
    pub(super) allow: Vec<EdgeRule>,
    pub(super) require: Vec<EdgeRule>,
    pub(super) budgets: Vec<Budget>,
    pub(super) waivers: Vec<Waiver>,
    pub(super) scenarios: Vec<Scenario>,
}

/// Small, fixed-depth decoding helpers used by the constraint grammar. Scenario
/// transforms are handled separately so nesting does not recurse through serde.
pub(super) mod decode {
    use super::*;
    use crate::architecture::compile::{
        policy::RunContext,
        yaml::{Kind, Value},
        CompileError,
    };
    use std::collections::HashMap;
    pub(in crate::architecture::compile) fn error(
        value: &Value,
        code: &'static str,
        reason: &str,
    ) -> CompileError {
        let mut error = CompileError::simple(code, reason);
        error.diagnostics[0].location = Some(value.location.clone());
        error
    }
    pub(in crate::architecture::compile) fn text(value: &Value) -> Result<&str, CompileError> {
        match &value.kind {
            Kind::String(s) => Ok(s),
            _ => Err(error(
                value,
                "architecture_invalid_input",
                "expected string",
            )),
        }
    }
    pub(in crate::architecture::compile) fn fields<'a>(
        value: &'a Value,
        required: &[&str],
        optional: &[&str],
        run: &RunContext,
    ) -> Result<HashMap<&'a str, &'a Value>, CompileError> {
        let Kind::Mapping(entries) = &value.kind else {
            return Err(error(
                value,
                "architecture_invalid_input",
                "expected mapping",
            ));
        };
        let mut output = HashMap::new();
        output
            .try_reserve(entries.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        for (key, value) in entries {
            run.check()?;
            let name = text(key)?;
            if !required.contains(&name) && !optional.contains(&name) {
                return Err(error(key, "unknown_field", "unknown field"));
            }
            output.insert(name, value);
        }
        if required.iter().any(|name| !output.contains_key(name)) {
            return Err(error(
                value,
                "architecture_invalid_input",
                "missing required field",
            ));
        }
        Ok(output)
    }
    pub(in crate::architecture::compile) fn items(value: &Value) -> Result<&[Value], CompileError> {
        match &value.kind {
            Kind::Sequence(items) => Ok(items),
            _ => Err(error(value, "architecture_invalid_input", "expected list")),
        }
    }
    pub(in crate::architecture::compile) fn id(value: &Value) -> Result<Id, CompileError> {
        Id::new(text(value)?).map_err(|_| error(value, "architecture_invalid_input", "invalid ID"))
    }
    pub(in crate::architecture::compile) fn nonempty(value: &Value) -> Result<Text, CompileError> {
        Text::new(text(value)?)
            .map_err(|_| error(value, "architecture_invalid_input", "empty text"))
    }
    pub(in crate::architecture::compile) fn integer(value: &Value) -> Result<u64, CompileError> {
        match &value.kind {
            Kind::Integer(n, false) => n.as_u64().ok_or_else(|| {
                error(
                    value,
                    "architecture_invalid_input",
                    "expected unsigned integer",
                )
            }),
            _ => Err(error(
                value,
                "architecture_invalid_input",
                "expected unsigned integer",
            )),
        }
    }
    pub(in crate::architecture::compile) fn digest(value: &Value) -> Result<Digest, CompileError> {
        Digest::new(text(value)?)
            .map_err(|_| error(value, "architecture_invalid_input", "invalid digest"))
    }
    pub(in crate::architecture::compile) fn level(value: &Value) -> Result<Level, CompileError> {
        match text(value)? {
            "component" => Ok(Level::Component),
            "container" => Ok(Level::Container),
            _ => Err(error(value, "architecture_invalid_input", "invalid level")),
        }
    }
    pub(in crate::architecture::compile) fn ids(
        value: &Value,
        run: &RunContext,
    ) -> Result<Vec<Id>, CompileError> {
        let values = items(value)?;
        let mut result = Vec::new();
        result
            .try_reserve(values.len())
            .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
        for value in values {
            run.check()?;
            result.push(id(value)?);
        }
        Ok(result)
    }
    pub(in crate::architecture::compile) fn projection(
        value: &Value,
        run: &RunContext,
    ) -> Result<ProjectionRef, CompileError> {
        let f = fields(value, &["version", "domain_ref"], &[], run)?;
        let version = text(f["version"])?;
        if version != "rust-q1/0.1" {
            return Err(error(
                f["version"],
                "unsupported_version",
                "unsupported projection version",
            ));
        }
        Ok(ProjectionRef {
            version: version.to_owned(),
            domain_ref: digest(f["domain_ref"])?,
        })
    }
    pub(in crate::architecture::compile) fn artifact(
        value: &Value,
        run: &RunContext,
    ) -> Result<ArtifactRef, CompileError> {
        use crate::architecture::{GitObjectId, RelativePath};
        let f = fields(
            value,
            &["repository", "commit", "path", "content_digest"],
            &[],
            run,
        )?;
        Ok(ArtifactRef {
            repository: nonempty(f["repository"])?,
            commit: GitObjectId::new(text(f["commit"])?)
                .map_err(|_| error(f["commit"], "architecture_invalid_input", "invalid commit"))?,
            path: RelativePath::new(text(f["path"])?)
                .map_err(|_| error(f["path"], "path_not_confined", "invalid path"))?,
            content_digest: digest(f["content_digest"])?,
        })
    }
    pub(in crate::architecture::compile) fn baseline(
        value: &Value,
        run: &RunContext,
    ) -> Result<BaselineRef, CompileError> {
        let f = fields(value, &["kind", "id"], &[], run)?;
        let id = digest(f["id"])?;
        match text(f["kind"])? {
            "input" => Ok(BaselineRef::Input { id }),
            "evaluation" => Ok(BaselineRef::Evaluation { id }),
            _ => Err(error(
                f["kind"],
                "architecture_invalid_input",
                "invalid baseline kind",
            )),
        }
    }
    pub(in crate::architecture::compile) fn edge(
        value: &Value,
        run: &RunContext,
    ) -> Result<EdgeRule, CompileError> {
        let f = fields(
            value,
            &["id", "level", "from", "to", "projection"],
            &[],
            run,
        )?;
        Ok(EdgeRule {
            id: id(f["id"])?,
            level: level(f["level"])?,
            from: ids(f["from"], run)?,
            to: ids(f["to"], run)?,
            projection: projection(f["projection"], run)?,
        })
    }
    pub(in crate::architecture::compile) fn budget(
        value: &Value,
        run: &RunContext,
    ) -> Result<Budget, CompileError> {
        let f = fields(
            value,
            &[
                "id",
                "level",
                "origins",
                "projection",
                "metric",
                "absolute_max",
                "max_increase",
                "baseline",
            ],
            &[],
            run,
        )?;
        let metric = match text(f["metric"])? {
            "cyclic_nodes" => Metric::CyclicNodes,
            "max_direct_fanout" => Metric::MaxDirectFanout,
            "max_transitive_reach" => Metric::MaxTransitiveReach,
            _ => {
                return Err(error(
                    f["metric"],
                    "architecture_invalid_input",
                    "invalid budget metric",
                ))
            }
        };
        Ok(Budget {
            id: id(f["id"])?,
            level: level(f["level"])?,
            origins: ids(f["origins"], run)?,
            projection: projection(f["projection"], run)?,
            metric,
            absolute_max: integer(f["absolute_max"])?,
            max_increase: integer(f["max_increase"])?,
            baseline: baseline(f["baseline"], run)?,
        })
    }
    pub(in crate::architecture::compile) fn clause(
        value: &Value,
        run: &RunContext,
    ) -> Result<OwnershipClause, CompileError> {
        let Kind::Mapping(entries) = &value.kind else {
            return Err(error(
                value,
                "architecture_invalid_input",
                "expected clause mapping",
            ));
        };
        let kind = entries
            .iter()
            .find_map(|(k, v)| {
                if matches!(&k.kind, Kind::String(s) if s == "kind") {
                    Some(v)
                } else {
                    None
                }
            })
            .ok_or_else(|| error(value, "architecture_invalid_input", "missing clause kind"))?;
        match text(kind)? {
            "exported_item" => {
                let f = fields(
                    value,
                    &["kind", "crate", "namespace", "item_path"],
                    &[],
                    run,
                )?;
                Ok(OwnershipClause::ExportedItem {
                    crate_name: nonempty(f["crate"])?,
                    namespace: nonempty(f["namespace"])?,
                    item_path: nonempty(f["item_path"])?,
                })
            }
            "reaches_exported_item" => {
                let f = fields(
                    value,
                    &["kind", "crate", "namespace", "item_path"],
                    &["max_hops"],
                    run,
                )?;
                let max_hops = f.get("max_hops").map(|v| integer(v)).transpose()?;
                if max_hops == Some(0) {
                    return Err(error(
                        f["max_hops"],
                        "architecture_invalid_input",
                        "max_hops must be positive",
                    ));
                }
                Ok(OwnershipClause::ReachesExportedItem {
                    crate_name: nonempty(f["crate"])?,
                    namespace: nonempty(f["namespace"])?,
                    item_path: nonempty(f["item_path"])?,
                    max_hops,
                })
            }
            _ => Err(error(
                kind,
                "architecture_invalid_input",
                "invalid clause kind",
            )),
        }
    }
    pub(in crate::architecture::compile) fn ownership(
        value: &Value,
        run: &RunContext,
    ) -> Result<OwnershipRule, CompileError> {
        let f = fields(
            value,
            &["id", "component", "responsibility_ids", "all_of", "none_of"],
            &[],
            run,
        )?;
        fn clauses(value: &Value, run: &RunContext) -> Result<Vec<OwnershipClause>, CompileError> {
            let mut output = Vec::new();
            let entries = items(value)?;
            output
                .try_reserve(entries.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            for entry in entries {
                run.check()?;
                output.push(clause(entry, run)?);
            }
            Ok(output)
        }
        Ok(OwnershipRule {
            id: id(f["id"])?,
            component: id(f["component"])?,
            responsibility_ids: ids(f["responsibility_ids"], run)?,
            all_of: clauses(f["all_of"], run)?,
            none_of: clauses(f["none_of"], run)?,
        })
    }
    fn utc_timestamp(value: &Value) -> Result<String, CompileError> {
        let s = text(value)?;
        let b = s.as_bytes();
        let invalid = || {
            error(
                value,
                "architecture_invalid_input",
                "expected quoted UTC RFC3339 timestamp",
            )
        };
        if !value.quoted
            || b.len() < 20
            || b[4] != b'-'
            || b[7] != b'-'
            || !matches!(b[10], b'T' | b't')
            || b[13] != b':'
            || b[16] != b':'
        {
            return Err(invalid());
        }
        fn number(bytes: &[u8]) -> Option<u32> {
            bytes.iter().try_fold(0u32, |n, b| {
                if b.is_ascii_digit() {
                    n.checked_mul(10)?.checked_add(u32::from(*b - b'0'))
                } else {
                    None
                }
            })
        }
        let year = number(&b[0..4]).ok_or_else(invalid)?;
        let month = number(&b[5..7]).ok_or_else(invalid)?;
        let day = number(&b[8..10]).ok_or_else(invalid)?;
        let hour = number(&b[11..13]).ok_or_else(invalid)?;
        let minute = number(&b[14..16]).ok_or_else(invalid)?;
        let second = number(&b[17..19]).ok_or_else(invalid)?;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err(invalid()),
        };
        if day == 0
            || day > days
            || hour > 23
            || minute > 59
            || second > 60
            || (second == 60
                && !(hour == 23
                    && minute == 59
                    && ((month == 6 && day == 30) || (month == 12 && day == 31))))
        {
            return Err(invalid());
        }
        let tail = &s[19..];
        let zone = if let Some(fraction) = tail.strip_prefix('.') {
            let n = fraction.bytes().take_while(u8::is_ascii_digit).count();
            if n == 0 {
                return Err(invalid());
            }
            &fraction[n..]
        } else {
            tail
        };
        if zone != "Z" && zone != "z" && zone != "+00:00" {
            return Err(invalid());
        }
        Ok(s.to_owned())
    }
    pub(in crate::architecture::compile) fn waiver(
        value: &Value,
        run: &RunContext,
    ) -> Result<Waiver, CompileError> {
        let f = fields(
            value,
            &[
                "id",
                "budget_id",
                "baseline",
                "original_limit",
                "ceiling",
                "expires_at",
                "approval_ref",
            ],
            &[],
            run,
        )?;
        let auth = fields(f["approval_ref"], &["reference", "context_id"], &[], run)?;
        Ok(Waiver {
            id: id(f["id"])?,
            budget_id: id(f["budget_id"])?,
            baseline: baseline(f["baseline"], run)?,
            original_limit: integer(f["original_limit"])?,
            ceiling: integer(f["ceiling"])?,
            expires_at: utc_timestamp(f["expires_at"])?,
            approval_ref: AuthorizationContextRef {
                reference: artifact(auth["reference"], run)?,
                context_id: id(auth["context_id"])?,
            },
        })
    }
    /// Returns a partially assembled document and the spanned scenario list.
    /// The caller must decode that list before publishing any successful model.
    pub(in crate::architecture::compile) fn document<'a>(
        value: &'a Value,
        entities: &mut u64,
        limit: u64,
        run: &RunContext,
    ) -> Result<(ConstraintDocument, &'a [Value]), CompileError> {
        let f = fields(
            value,
            &[
                "schema_version",
                "ownership_rules",
                "allow",
                "require",
                "budgets",
                "waivers",
                "scenarios",
            ],
            &[],
            run,
        )?;
        let schema_version = SchemaVersion::new(text(f["schema_version"])?).map_err(|_| {
            error(
                f["schema_version"],
                "unsupported_version",
                "unsupported schema version",
            )
        })?;
        fn decode_list<T>(
            value: &Value,
            entities: &mut u64,
            limit: u64,
            run: &RunContext,
            decode: fn(&Value, &RunContext) -> Result<T, CompileError>,
        ) -> Result<Vec<T>, CompileError> {
            let values = items(value)?;
            let count = u64::try_from(values.len())
                .map_err(|_| CompileError::simple("resource_limit", "entity count overflow"))?;
            *entities = entities
                .checked_add(count)
                .ok_or_else(|| CompileError::simple("resource_limit", "entity count overflow"))?;
            if *entities > limit {
                return Err(CompileError::simple(
                    "resource_limit",
                    "authored entity limit exceeded",
                ));
            }
            let mut out = Vec::new();
            out.try_reserve(values.len())
                .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
            for value in values {
                run.check()?;
                out.push(decode(value, run)?);
            }
            Ok(out)
        }
        let doc = ConstraintDocument {
            schema_version,
            ownership_rules: decode_list(f["ownership_rules"], entities, limit, run, ownership)?,
            allow: decode_list(f["allow"], entities, limit, run, edge)?,
            require: decode_list(f["require"], entities, limit, run, edge)?,
            budgets: decode_list(f["budgets"], entities, limit, run, budget)?,
            waivers: decode_list(f["waivers"], entities, limit, run, waiver)?,
            scenarios: Vec::new(),
        };
        Ok((doc, items(f["scenarios"])?))
    }
}

impl ProjectionRef {
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn domain_ref(&self) -> &Digest {
        &self.domain_ref
    }
}
impl AuthorizationContextRef {
    pub fn reference(&self) -> &ArtifactRef {
        &self.reference
    }
    pub fn context_id(&self) -> &Id {
        &self.context_id
    }
}
impl OwnershipRule {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn component(&self) -> &Id {
        &self.component
    }
    pub fn responsibility_ids(&self) -> &[Id] {
        &self.responsibility_ids
    }
    pub fn all_of(&self) -> &[OwnershipClause] {
        &self.all_of
    }
    pub fn none_of(&self) -> &[OwnershipClause] {
        &self.none_of
    }
}
impl EdgeRule {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn level(&self) -> Level {
        self.level
    }
    pub fn from(&self) -> &[Id] {
        &self.from
    }
    pub fn to(&self) -> &[Id] {
        &self.to
    }
    pub fn projection(&self) -> &ProjectionRef {
        &self.projection
    }
}
impl Budget {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn level(&self) -> Level {
        self.level
    }
    pub fn origins(&self) -> &[Id] {
        &self.origins
    }
    pub fn projection(&self) -> &ProjectionRef {
        &self.projection
    }
    pub fn metric(&self) -> &Metric {
        &self.metric
    }
    pub fn absolute_max(&self) -> u64 {
        self.absolute_max
    }
    pub fn max_increase(&self) -> u64 {
        self.max_increase
    }
    pub fn baseline(&self) -> &BaselineRef {
        &self.baseline
    }
}
impl Waiver {
    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn budget_id(&self) -> &Id {
        &self.budget_id
    }
    pub fn baseline(&self) -> &BaselineRef {
        &self.baseline
    }
    pub fn original_limit(&self) -> u64 {
        self.original_limit
    }
    pub fn ceiling(&self) -> u64 {
        self.ceiling
    }
    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }
    pub fn approval_ref(&self) -> &AuthorizationContextRef {
        &self.approval_ref
    }
}
impl ConstraintDocument {
    pub fn schema_version(&self) -> &SchemaVersion {
        &self.schema_version
    }
    pub fn ownership_rules(&self) -> &[OwnershipRule] {
        &self.ownership_rules
    }
    pub fn allow(&self) -> &[EdgeRule] {
        &self.allow
    }
    pub fn require(&self) -> &[EdgeRule] {
        &self.require
    }
    pub fn budgets(&self) -> &[Budget] {
        &self.budgets
    }
    pub fn waivers(&self) -> &[Waiver] {
        &self.waivers
    }
    pub fn scenarios(&self) -> &[Scenario] {
        &self.scenarios
    }
}
