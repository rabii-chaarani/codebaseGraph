#![allow(dead_code)]
#[path = "/private/tmp/cleanup-v2-s3-review-_098ctam/repo/src/storage/maintenance_budget.rs"]
mod maintenance_budget;
fn needs_send<T: Send>() {}
fn needs_sync<T: Sync>() {}
fn main() {
    needs_send::<maintenance_budget::MutationGrant>();
    needs_sync::<maintenance_budget::MutationGrant>();
}
