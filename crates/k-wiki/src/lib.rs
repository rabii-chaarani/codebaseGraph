//! OKF knowledge wiki.
//!
//! The crate keeps OKF source semantics separate from the source graph and
//! exposes one normalized projection to every transport.

pub mod adapters;
pub mod api;
pub mod authoring;
pub mod bundle;
pub mod compiler;
pub mod conformance;
pub mod diagnostic;
pub mod graph_context;
pub mod model;
pub mod projection;
pub mod refresh;
pub mod render;
pub mod search;
pub mod service;

pub const WIKI_SCHEMA_VERSION: u32 = 1;
