# COD-53 final review — D05d default-extension subset

## Verdict

**PROCEED for the reviewed subset as a draft PR. No confirmed introduced P0–P3 defects were found.** This is a source/configuration/test/docs review, not S02 completion or Windows runtime/source-install qualification. A03/A04/A16 and full COD-53 closure remain pending. No product fixes were needed, so there is no fix-verification round to report.

## Reviewer and exact snapshot

- Fresh independent reviewer: **GPT-6.1 Sol**, actual resolved model **`openai-codex/gpt-6.1-sol`**, **`thinking="max"`**, fresh session with no implementation-history fork; spawn settings confirmed by coordinator.
- Applied `/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md` and its engineering lenses. Reviewed approval/spec, fresh Sol High proposal review, actual baseline/current source and frozen diff, adjacent feature/call paths, package and executed evidence.
- Worktree: `/private/tmp/codebasegraph-cod53-windows-runtimes`.
- Source baseline: **`7038e66ff8271ffddc511f879cf8d28353a09b2d`**.
- Approved canonical spec: `docs/specs/CBG-PLAT-01/spec.md`, **rev0.10**, **`1a9db7b558b60248888408daa6268a3b45f2f0df`**, S02/R02/A03/A04/A16; this implementation/review covers only the separable D05d R02/A16 configuration/test/docs subset.
- Approval record: **`eaa430c849094078bb4c228ea03136dde5587e91`**:`docs/specs/CBG-PLAT-01/approvals/cod53-windows-native-runtimes.md`.
- Latest docs-only HEAD during final recheck: **`389f86f58a03f71d81b0a2aeb44e7cc49a58f4ce`** (Community2022 route record). Its additional docs do not change the frozen product diff. Pending execution-doc additions are not an expansion of this product review.
- Frozen 3-file diff SHA256: **`fee7b0a6122950e952eb79758d3dadf58ecceea9389e7bffc4869090da096d82`**. Independently reproduced the exact patch against the baseline. All eight file identities in `subset-source-snapshot.json` matched, including unchanged lock/wiki/xtask/installer/setup files; rechecked at the end.

| Reviewed file | SHA256 |
| --- | --- |
| `Cargo.toml` | `455a508cf5a3d4ac664af80133bafebe1c8b4c4f721d3ca3e52ab65f822b298e` |
| `docs/release.md` | `6343cbe12df65148254a057cd65572631d8f3b2c0a49029eaef5c696729740d9` |
| `src/db_writer/extensions.rs` | `fdaad58193a79b94345a88237efe452938f643ee9c45b8c6e8548c36b760a69d` |

## Consequential paths checked

1. **Default inclusion and opt-out.** `Cargo.toml:38–40` adds only `default = ["bundled-windows-extensions"]`. The unchanged Windows/x86_64/feature predicate at `src/db_writer/extensions.rs:98–124` selects the existing two `include_bytes!` assets. Other platform predicates, Ladybug/cache version0.19.0, dependency versions and bytes are unchanged. The feature's globally enabled name does not broaden compiled platform assets or supported targets. The root-only `--no-default-features` invocation still excludes the feature.
2. **Wiki dependency propagation.** `crates/k-wiki/Cargo.toml:18–27` retains forwarding and its root dependency without `default-features = false`; its build inherits root defaults. It remains `publish = false`, as the docs state. Workspace feature unification can reactivate root defaults through wiki: a workspace metadata run with `--no-default-features` is **not** proof of an isolated root opt-out. The supplied opt-out metadata note correctly says this; the root-selected tree provides the separate check.
3. **Regression assertion.** `src/db_writer/extensions.rs:142–155` now fails if either JSON or FTS returns `None` on feature-enabled Windows x86_64, then uses the existing real XZ decompression and length check. Explicit Windows opt-out remains legal. This catches silent absence inside the enabled branch; by itself it does not detect removal of the Cargo default, and the current forced-feature Windows CI cannot supply that proof. Default resolution and a future no-extra-feature Windows execution are separate evidence.
4. **Runtime scope/failures.** Writer preparation still propagates preseed errors (`src/db_writer/write.rs:173–184`), and search propagates preseed/`LOAD fts` failures (`src/api/graph_read.rs:88–95`). No production extension loader/cache write, DLL policy, installer, archive inventory, NativeTarget, setup or ABI code changed. Missing-runtime behavior and clean-consumer safety are not newly demonstrated.
5. **Documentation.** `docs/release.md:119–152` distinguishes bundled bytes from compiler/OpenSSL/VC prerequisites, identifies unpublished wiki correctly, requires real supported Windows11 source-install/empty-cache/offline JSON/FTS/build/reopen/search evidence, and expressly declines current prebuilt or exact-source-recipe qualification. Its named DLL prerequisites match both existing Windows PE import tables. lbug0.19.0's published `build.rs` confirms Windows CMake/Ninja fallback; current root `build.rs` requests the OpenSSL link libraries. No new source-build recipe is asserted.

## Verification evidence

**Reviewer-executed read-only checks:** offline locked Windows-filtered Cargo metadata (no `--features`) resolved root `default` and `bundled-windows-extensions`; isolated `cargo tree --offline --locked -p codebase-graph --no-default-features -e features -i codebase-graph` showed neither feature. Inspected the actual final `.crate`, its normalized default features, included final test source and four XZ files. All four XZ hashes match both worktree and baseline. Independently decompressed both Windows XZ files and read x64 PE/import headers: this is static artifact inspection, **not Windows execution**.

**Coordinator-executed results inspected in logs/JUnit, not rerun by reviewer:**

| Check | Actual result and limit |
| --- | --- |
| Default macOS ARM64 extension/JSON COPY/FTS search cohort | 4/4 passed; 838 filtered; root compile line names target worktree |
| Root explicit-no-default extension cohort | 2/2 passed; 840 filtered; macOS does not execute the Windows branch |
| xtask | 42/42 passed; zero skipped; JUnit inspected |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed; no warnings in log |
| `cargo package --locked --allow-dirty` | Normal extracted root package compile passed on macOS with supplied verified0.19 native library; not Windows source install or a fresh native-library source build |

Final archive independently rehashed: **8,451,673 bytes**, below **10,485,760**, SHA256 **`e843e817b7c9da071d1983d756c1eb90e7034a96f886bcfeafd15e2fe0a73132`**. It matches `subset-final-package-measure.json`; extracted source hash matches the frozen final test file. Earlier partial archive evidence is not substituted for this final artifact. The final archive checks establish package inventory/size and macOS extracted compilation only.

Evidence: `/private/tmp/codebasegraph-cod53-analysis/subset-verification-evidence.json`, final package measure/log, focused/opt-out/xtask logs and JUnit. Coordinator has copied these under `docs/specs/CBG-PLAT-01/execution/cod53-default-extensions-evidence/` for durable delivery.

## Remaining gates / limitations — not introduced defects

- No Windows native default-build test or plain Windows11 source install ran. The Windows assertion is reviewed statically; metadata and macOS tests do not establish A16. A no-extra-feature Windows test plus the specified real source-install/offline behavior remains required before qualification.
- No clean Windows11 prebuilt/installer/DLL success/failure execution or final runtime artifact closure was reviewed. A03/A04 and full S02 remain OPEN; this is not a release, merge, publication or aggregate-closure recommendation.
- The user has now selected the **Visual Studio Community2022** licence route in `389f86f58a03f71d81b0a2aeb44e7cc49a58f4ce:docs/specs/CBG-PLAT-01/decisions/cod53-vc-runtime-licence.md`; it is **not an unanswered route question or a paid-licence requirement**. Eligible acquisition/use and exact permitted unmodified DLL provenance/notices, compatibility and controlled-loading evidence remain necessary before copying/shipping VC files. This subset adds no VC DLLs.
- Root AGENTS was supplied/read; the isolated worktree has no AGENTS file. Recalled repository memory, read wiki release context, fetched Scryer rules/orientation and `chg-p08sev`; `resp-113` is the existing claim. Configured graph/wiki/Scryer bind the older main checkout: graph discovery/context supplied orientation, while actual hashes/source/package supplied revision evidence. TOML manifests are absent from graph File nodes; graph-discovered xtask evidence identifies their paths. Linear connection is pending/unavailable; no tracking or Scryer verdict/closure mutation was made. Stale four-target wiki prose was not treated as current scope.
- No product/tracking/Git edits, builds, tests or CI executions were performed by this reviewer. Only this report was written. Additional independent primary references opened2026-10-27: [Cargo default features](https://doc.rust-lang.org/cargo/reference/features.html#the-default-feature), [cargo install feature options](https://doc.rust-lang.org/cargo/commands/cargo-install.html#feature-selection), [published lbug0.19.0 build script](https://docs.rs/crate/lbug/0.19.0/source/build.rs) (matches installed dependency source).
- Reusable-knowledge assessment: no new durable memory warranted. This is a task-specific incomplete qualification state; existing Cargo-feature/package/native-profile knowledge already covers the reusable mechanics.

The only production setting changed is the root default feature. No numeric constants changed. This recommendation assumes delivery remains explicitly labelled as this reviewed subset; it does not assume runtime entitlement, supported-platform execution or full acceptance.
