mod command;
mod request;

pub(crate) use crate::api::materialization::MaterializeOptions;
pub(in crate::adapters::cli) use command::{run_materialize, run_plan};
