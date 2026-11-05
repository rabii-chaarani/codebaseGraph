# CBG-COPY-01 — Execution status

## Authority and snapshot

Approved [Scope A](spec.md), revision 0.2 at full specification commit `c5b62ba77d2f94b71714b5319f3b3fa215983f9f`; [approval](approval.md) quotes the user's explicit implementation/PR request. Implementation worktree starts at existing PR153 head `5839055d9279f4475f893a54abeaa542344c1b02` and preserves its supported-platform policy (macOS ARM64, Windows x86_64). [Independent baseline adaptation review](baseline-review.md) is green; a fresh final code review is still required.

Scryer original planning ledger `chg-5qnevq` and execution ownership/brief `chg-v5863h` record the same-scope user sign-off. No new architecture claim or responsibility was invented for this existing-claim safety fix. The registered Scryer/CodebaseGraph services refer to the original checkout. A Scryer read scoped to this new worktree reports no model; do not ingest current test reports under old source fingerprints or claim global drift was reconciled. Linear remains verification-pending with no dispatchable issue tools; synchronization is pending.

## Executed baseline preparation

- Toolchain: Rust/Cargo 1.96.0, macOS arm64.
- Official Ladybug 0.19.0 static archive: SHA-256 `6bee40babdf837bb6aa4d271047dfe5ed3dacf022f16af84952ca4c22d034d62`, matching official release metadata; paired headers/static library provided explicitly through `LBUG_LIBRARY_DIR`/`LBUG_INCLUDE_DIR`, with `LBUG_VERSION=0.19.0`. No dependency upgrade or Cargo.lock update.
- Baseline `cargo test -p codebase-graph --lib --tests --locked --no-run`: passed. This compiled existing targets only; it did not execute acceptance tests.

## Implementation and verification

In progress. Local focused tests passed (72/72), the real CLI COPY regressions passed (2/2), release-recipe tests passed (42/42), and strict workspace/all-target/all-feature Clippy passed after correcting one item-order violation. The complete serial workspace nextest run has started with 1,344 cases; it is not yet a pass. See [local check index](evidence/local-checks-in-progress.json) for provenance and early failed compile/test-oracle/lint attempts; failures were corrected rather than hidden.

The native regression writes through a source-bound normal CLI with isolated phase children. It covers Managed and Direct unchanged database/manifest/search bundles after actual COPY+checkpoint success followed by an injected acknowledgement error, then a separate successful invocation. The chunk fixture uses the configured 1 MiB spill/final bound, checks early indexes, reopens in a new process, compares all 1,400 expected Function labels and keyed lookups, probes every ID in repeated node/edge-node tables, confirms the native `PRIMARY_KEY_SCAN_NODE_TABLE` operator, verifies exact Module→Defines→Function endpoints, and searches across chunked output. Observed database phase charge was 130,073,800 bytes in that fixture; this is a local sample, not a general peak bound or platform guarantee.

The initial connector oracle expected `HasReturnType`, which this Rust fixture does not emit. Read-only scratch queries established the actual `Defines` edges; the corrected test passed. No product behavior was changed to satisfy that mistaken oracle. The full suite, final review, native profile and Windows CI remain separate gates. No merge, deployment, daemon restart or live graph rewrite is authorized.
