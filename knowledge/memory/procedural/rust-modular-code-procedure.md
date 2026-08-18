---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-08-11T14:08:00+09:30
  last_verified_at: 2026-08-11T14:08:32+09:30
  verified_by: repository-agents
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: rust-book
    reference: https://doc.rust-lang.org/stable/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html
    content_hash: null
  - kind: rust-book
    reference: https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html
    content_hash: null
  - kind: rust-reference
    reference: https://doc.rust-lang.org/reference/visibility-and-privacy.html
    content_hash: null
  - kind: rust-api-guidelines
    reference: https://rust-lang.github.io/api-guidelines/checklist.html
    content_hash: null
  - kind: cargo-reference
    reference: https://doc.rust-lang.org/cargo/reference/workspaces.html
    content_hash: null
  - kind: cargo-reference
    reference: https://doc.rust-lang.org/stable/cargo/reference/features.html
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-08-11T14:08:32+09:30
    reason: Reviewed against the cited official Rust module and visibility documentation, Rust API Guidelines, and Cargo workspace and feature references. The procedure accurately covers domain-oriented modules, narrow visibility, invariant ownership, trait and crate boundaries, additive features, and layered verification.
description: A repeatable workflow for organizing Rust modules, APIs, dependency boundaries, traits, crates, features, and tests.
tags:
- api-design
- modularity
- modules
- rust
- testing
- traits
- workspaces
timestamp: 2026-08-11T14:08:00+09:30
title: Procedure for modular Rust code
type: agent-memory
---
Use this procedure when designing or refactoring Rust code for clear responsibility, replaceable boundaries, and maintainable public APIs.

1. Identify capabilities and ownership boundaries before choosing files. Organize modules around domain features or use cases, keeping each feature's model, validation, operations, and errors close together. Avoid vague dumping grounds such as `utils`, `helpers`, or global `models` unless their scope is genuinely coherent.
2. Start with modules in one crate. Let the module tree express responsibility and privacy; split modules into files only as they grow. Keep `lib.rs` as a curated facade and `main.rs` as a thin composition root that loads configuration, constructs dependencies, and starts the application.
3. Keep the smallest useful visibility. Default to private, then widen deliberately with `pub(super)`, `pub(crate)`, or `pub`. Re-export intentional public names with `pub use` so callers depend on the stable facade rather than internal paths.
4. Make modules own their invariants. Prefer private fields, validated constructors, newtypes, and domain methods over publicly mutable data. Use `#[non_exhaustive]` or sealed traits when a public API needs controlled future evolution.
5. Make dependencies explicit through constructors and function parameters. Avoid hidden globals and service locators. Keep pure domain calculations and state transitions separate from databases, networks, clocks, files, and framework adapters.
6. Introduce traits at meaningful boundaries: multiple implementations, infrastructure adapters, consumer-supplied behavior, or valuable test substitution. Define the smallest behavior the consumer needs. Do not create a trait for every concrete type.
7. Choose abstraction forms deliberately: an enum for a closed implementation set, a trait for an open extension point, generics for static dispatch, and `dyn Trait` for runtime-selected or heterogeneous implementations. Consider API stability, compile time, code size, and object-safety constraints.
8. Keep dependency direction toward stable domain concepts. Application/use-case services may depend on domain types and narrow ports; infrastructure implements those ports. Break architectural cycles by moving ownership or extracting the smallest shared contract, not by creating a miscellaneous common module.
9. Define errors at module boundaries in terms meaningful to the caller. Translate infrastructure errors at the adapter or application boundary instead of leaking backend-specific error types through domain APIs.
10. Extract a separate crate only for a real independent boundary: reuse, distinct dependencies or platform requirements, independent ownership/release, or a deliberately stable API. Use a Cargo workspace when related packages evolve together. Avoid microcrate fragmentation used only for file organization.
11. Keep Cargo features additive, orthogonal, documented, and safe in any combination because Cargo unifies enabled features. Prefer runtime configuration or separate crates when choices are mutually exclusive.
12. Test each boundary at its appropriate level: unit/property tests beside private invariants and pure logic, contract tests shared by trait implementations, integration tests through the public API, and rustdoc examples for public usage.
13. Before completing a modularity change, verify that public behavior is preserved or intentionally changed, unwanted visibility has not expanded, dependencies still point in the intended direction, all feature combinations compile as required, and workspace formatting, linting, tests, documentation tests, and release build pass.