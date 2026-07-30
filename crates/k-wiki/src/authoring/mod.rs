//! Controlled OKF bundle and page authoring.

mod bundle;
mod page;
mod write;

pub use bundle::{AuthoringConfig, AuthoringService, BundleRoot, RepositoryRoot};
pub use page::{
    CreateBundleRequest, CreateBundleResult, CreatePageRequest, CreatePageResult, PageFrontmatter,
    PopulatePageRequest, PopulatePageResult,
};
pub use write::{
    AuthoringError, AuthoringValidator, ConformanceAuthoringValidator, NoopRefreshNotifier,
    NoopValidator, RefreshEvent, RefreshNotifier, RefreshOperation, ValidationTarget,
    ValidationTargetKind,
};
