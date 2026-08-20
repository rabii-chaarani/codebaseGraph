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
  - kind: rustonomicon
    reference: https://doc.rust-lang.org/stable/nomicon/safe-unsafe-meaning.html
    content_hash: null
  - kind: rustonomicon
    reference: https://doc.rust-lang.org/stable/nomicon/working-with-unsafe.html
    content_hash: null
  - kind: rust-book
    reference: https://doc.rust-lang.org/stable/book/ch09-03-to-panic-or-not-to-panic.html
    content_hash: null
  - kind: rustsec
    reference: https://rustsec.org/
    content_hash: null
  - kind: rust-fuzz
    reference: https://rust-fuzz.github.io/book/cargo-fuzz.html
    content_hash: null
  - kind: rust-stdlib
    reference: https://doc.rust-lang.org/stable/std/process/index.html
    content_hash: null
  history:
  - from: candidate
    to: active
    actor: repository-agents
    at: 2026-08-11T11:39:37+09:30
    reason: Reviewed against the cited Rustonomicon safe/unsafe contracts, official Rust error-handling and process-boundary guidance, RustSec dependency auditing guidance, and Rust fuzzing documentation; the procedure preserves defense-in-depth beyond memory safety.
description: A defense-in-depth workflow covering safe Rust, untrusted inputs, unsafe boundaries, dependencies, secrets, concurrency, and security verification.
tags:
- dependencies
- fuzzing
- input-validation
- rust
- security
- unsafe
timestamp: 2026-08-11T11:38:34+09:30
title: Procedure for secure Rust development
type: agent-memory
---
Use this procedure for Rust code that processes external data, crosses privilege or FFI boundaries, handles secrets, or is exposed to untrusted users.

1. Define assets, attackers, trust boundaries, privileges, and abuse cases. Include denial of service and supply-chain compromise; memory safety alone does not prevent injection, authorization flaws, secret leakage, or logical races.
2. Prefer safe Rust and enforce `#![forbid(unsafe_code)]` in crates that need no unsafe code. Remember this does not constrain dependencies.
3. If unsafe or FFI is required, isolate it in a small private module. Document every safety invariant, ownership/lifetime rule, alignment/nullability requirement, integer width, thread-safety promise, and unwind boundary. Expose a safe API that cannot violate those invariants.
4. Treat all network, file, environment, database, command-line, deserialized, and FFI values as hostile. Validate before allocation or arithmetic; cap bytes, counts, nesting, recursion, decompression, concurrency, and duration. Use `TryFrom`, checked arithmetic, safe slicing, and `try_reserve` where allocation failure is recoverable.
5. Encode validated states and authorization decisions in types or constructors. Check object-level authorization at the action point and fail closed when validation or policy evaluation fails.
6. Return `Result` for malformed or adversarial input. Avoid `unwrap`, `expect`, assertions, and indexing on attacker-reachable paths. Do not expose stack traces, internal paths, SQL errors, or secrets.
7. Prevent injection with structured boundaries: parameterized queries, context-aware output encoding, and `Command` with separate arguments rather than a shell. Use fixed executable paths and controlled environments. Treat Windows batch/cmd argument handling specially.
8. Defend filesystem operations against traversal, symlinks, hard links, and TOCTOU races. Canonicalization and prefix checks are insufficient when an attacker can mutate the tree; use descriptor-relative/no-follow operations for hostile directories.
9. Minimize and review dependencies, features, build scripts, proc macros, native code, and transitive unsafe code. Commit `Cargo.lock` for applications, patch promptly, run `cargo audit` and policy checks such as `cargo deny`, and preserve auditable dependency metadata. Known-advisory scans do not replace review.
10. Use established high-level cryptography and secure randomness. Never log secrets; use redacting secret types, minimize copies, validate TLS identity, and use constant-time comparisons where required. Zeroization reduces exposure but is not a complete erasure guarantee.
11. Remember safe Rust prevents data races, not deadlocks, logical races, TOCTOU, or unbounded resource consumption. Use bounded queues/concurrency, deadlines, cancellation, atomic state transitions where required, and a documented lock order.
12. Verify hostile behavior with boundary and authorization tests, property tests, fuzzing, Miri for unsafe code, and platform sanitizers. CI should run formatting, strict Clippy, tests, dependency/advisory policy, and relevant fuzz targets.
13. Deploy with least privilege, restrictive filesystem/network access, timeouts, resource limits, rate limits, secret management, and safe diagnostic logging.