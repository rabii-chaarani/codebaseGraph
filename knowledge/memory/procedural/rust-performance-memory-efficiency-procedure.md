---
agent_memory:
  version: 1
  kind: procedural
  scope: repository
  status: active
  owner: repository-agents
  created_at: 2026-08-11T11:38:34+09:30
  last_verified_at: 2026-08-11T11:39:37+09:30
  verified_by: repository-agents
  review_after: null
  supersedes: []
  superseded_by: null
  sources:
  - kind: documentation
    reference: https://nnethercote.github.io/perf-book/general-tips.html
    content_hash: null
  - kind: documentation
    reference: https://nnethercote.github.io/perf-book/heap-allocations.html
    content_hash: null
  - kind: rust-stdlib
    reference: https://doc.rust-lang.org/stable/std/vec/struct.Vec.html
    content_hash: null
  - kind: cargo-reference
    reference: https://doc.rust-lang.org/cargo/reference/profiles.html
    content_hash: null
  - kind: rust-stdlib
    reference: https://doc.rust-lang.org/stable/std/mem/fn.size_of.html
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-08-11T11:39:37+09:30
    reason: Reviewed against the cited Rust Performance Book guidance, standard-library Vec and size_of documentation, and Cargo profile reference; the procedure accurately separates measurement, allocation/layout optimization, build tuning, and verification.
description: A measurement-first workflow for improving Rust runtime speed, allocation behavior, peak memory, and data locality.
tags:
- memory-efficiency
- optimization
- performance
- profiling
- rust
timestamp: 2026-08-11T11:38:34+09:30
title: Procedure for optimized, memory-efficient Rust
type: agent-memory
---
Use this procedure when writing or optimizing Rust where runtime speed or memory usage matters.

1. Define the measurable objective: latency/throughput, allocation count, peak RSS, retained capacity, or binary size. Choose representative workloads and preserve correctness tests.
2. Measure a release build before changing code. Profile CPU and allocations; inspect hot allocation sites, peak memory, and frequently instantiated type sizes with `size_of`.
3. Fix algorithms and data structures first. Prefer contiguous storage such as slices, arrays, and `Vec<T>`; avoid pointer-heavy layouts unless required. Choose array-of-structures versus structure-of-arrays according to actual access patterns.
4. Reduce allocation and copying on measured hot paths: accept `&str`/`&[T]`, move rather than clone, avoid intermediate `collect`, preallocate known capacity, and reuse scratch `String`/`Vec` buffers with `clear`. Use `Cow`, `clone_from`, small-vector/string types, or alternate allocators only when measurements justify them.
5. Control retained memory. Validate size estimates, reserve deliberately, and use `shrink_to_fit` or boxed slices only for long-lived over-capacity collections—not repeatedly in loops.
6. Inspect hot data layout. Compact overly wide fields where semantically safe; measure padding and enum size. Box a rare oversized enum variant only when the reduced common-case footprint outweighs its extra allocation. Do not use packed layout merely to save bytes.
7. Avoid accidental quadratic work: front removal from `Vec`, repeated string concatenation, repeated reallocations, or unnecessary element shifting. Prefer suitable collections and bulk/in-place operations.
8. Experiment with release profile settings such as `lto = "thin"`, fewer codegen units, and relevant optimization levels. Treat runtime speed, runtime memory, and binary size as separate goals with trade-offs.
9. Re-run benchmarks and memory profiles on representative workloads. Keep a change only when it produces a material, repeatable improvement without correctness regression.
10. Use `unsafe`, unchecked indexing, SIMD, or allocator replacement only after profiling proves need. Document invariants and verify unsafe code with focused tests, Miri, and sanitizers where applicable.