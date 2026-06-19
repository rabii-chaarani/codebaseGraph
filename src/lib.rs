pub mod cli;
pub mod db_writer;
pub mod error;
mod execution;
mod graph_rows;
mod hash;
mod normalize;
mod parser;
mod partition_builder;
mod profiles;
pub mod protocol;
mod scan;
mod semantic_enrichment;
mod staging_writer;
mod syntax_materializer;

pub use execution::{
    materialize_syntax_batch, materialize_syntax_batch_json, plan_syntax_materialization,
};
