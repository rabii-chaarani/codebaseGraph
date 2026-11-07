use super::CompileError;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

macro_rules! policy_fields {
    ($m:ident) => {
        $m!(
            max_closure_files,
            max_file_bytes,
            max_yaml_bytes,
            max_parse_depth,
            max_authored_entities,
            max_expanded_rule_pairs,
            max_mapping_records,
            max_citations_per_mapping,
            max_rationale_bytes,
            max_proposal_operations,
            max_scenarios,
            max_projected_vertices,
            max_projected_edges,
            max_traversal_steps,
            max_evidence_bytes,
            max_result_bytes,
            max_evaluation_duration_ms
        );
    };
}
macro_rules! define_policy {
    ($($field:ident),*) => {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(deny_unknown_fields, try_from = "RawPolicy")]
        pub struct ArchitectureExecutionPolicy { $(pub(crate) $field: u64),* }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawPolicy { $($field: u64),* }
        impl TryFrom<RawPolicy> for ArchitectureExecutionPolicy {
            type Error = &'static str;
            fn try_from(v: RawPolicy) -> Result<Self, Self::Error> {
                if $(v.$field == 0)||* { return Err("all policy values must be positive"); }
                Ok(Self { $($field: v.$field),* })
            }
        }
        impl ArchitectureExecutionPolicy {
            $(pub fn $field(&self) -> u64 { self.$field })*
            /// Construct a policy from every required field, in the documented field order.
            pub fn from_json(value: serde_json::Value) -> Result<Self, CompileError> {
                serde_json::from_value(value).map_err(|e| CompileError::simple("architecture_configuration_required", e.to_string()))
            }
            pub(crate) fn effective(&self, o: Option<&ArchitectureExecutionPolicyOverride>) -> Result<Self, CompileError> {
                let mut p = self.clone();
                if let Some(o) = o { $(if let Some(v) = o.$field { if v == 0 || v > p.$field { return Err(CompileError::simple("architecture_configuration_required", concat!("invalid override: ", stringify!($field)))); } p.$field = v; })* }
                Ok(p)
            }
        }
        #[derive(Clone, Debug, Default, Deserialize)]
        #[serde(try_from = "RawOverride")]
        pub struct ArchitectureExecutionPolicyOverride { $($field: Option<u64>),* }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawOverride { $($field: Option<u64>),* }
        impl TryFrom<RawOverride> for ArchitectureExecutionPolicyOverride {
            type Error = &'static str;
            fn try_from(raw: RawOverride) -> Result<Self, Self::Error> {
                if $(raw.$field == Some(0))||* { return Err("override values must be positive"); }
                Ok(Self { $($field: raw.$field),* })
            }
        }
    }
}
policy_fields!(define_policy);

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}
impl ExecutionContext {
    pub fn new(cancelled: Arc<AtomicBool>, deadline: Option<Instant>) -> Self {
        Self {
            cancelled,
            deadline,
        }
    }
    pub fn cancellation(&self) -> &Arc<AtomicBool> {
        &self.cancelled
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    #[cfg(test)]
    pub(crate) fn start(
        &self,
        p: &ArchitectureExecutionPolicy,
    ) -> Result<RunContext, CompileError> {
        self.start_at(p, Instant::now())
    }
    pub(crate) fn start_at(
        &self,
        p: &ArchitectureExecutionPolicy,
        start: Instant,
    ) -> Result<RunContext, CompileError> {
        // An unrepresentable distant configured deadline is later than every
        // representable caller deadline; it is not an invalid u64 policy value.
        let configured = start.checked_add(Duration::from_millis(p.max_evaluation_duration_ms));
        let deadline = match (self.deadline, configured) {
            (Some(caller), Some(configured)) => Some(caller.min(configured)),
            (Some(caller), None) => Some(caller),
            (None, configured) => configured,
        };
        Ok(RunContext {
            cancelled: self.cancelled.clone(),
            deadline,
        })
    }
}
pub(crate) struct RunContext {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}
impl RunContext {
    pub(crate) fn check(&self) -> Result<(), CompileError> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(CompileError::simple("cancelled", "compilation cancelled"));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(CompileError::simple(
                "deadline_exceeded",
                "compilation deadline exceeded",
            ));
        }
        Ok(())
    }
}
