mod parallel;
mod plan;
mod run;
mod timing;

pub use plan::plan_materialization;
pub use run::execute_materialization_pipeline;
