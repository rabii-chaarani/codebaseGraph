# Native environment readiness — PR163 START-only repair

**Status:** existing native dependency validated; ready for parent-assigned baseline/candidate build slot. No builds/tests, downloads, installs or source/settings edits performed. User-approved START-only acknowledgement 250ms→5s repair is now recorded by coordinator under `chg-1k9jre`; independent evidence remains pending candidate assignment.

## Validated dependency

Tracing only prior COD54 Cargo build outputs found the existing external native directory:
`/private/tmp/codebasegraph-copy153-evidence/native019`.

- Cached official archive: `/private/tmp/codebasegraph-copy153-evidence/liblbug-static-osx-arm64.tar.gz`.
- Archive SHA256: `6bee40babdf837bb6aa4d271047dfe5ed3dacf022f16af84952ca4c22d034d62`, size22549116.
- Live read-only `gh api repos/LadybugDB/ladybug/releases/tags/v0.19.0` returned the matching official asset URL, SHA256 digest and size (asset494781478). No asset download occurred. Raw response saved in `ladybug-official-release-api.json`.
- Archive has exactly three regular members. Every current extracted member matches its archived bytes:
  - `liblbug.a`: `d2e9a5c37617b0affb01f18f6ef48bb1531f5744f5ba06c849226842617ad142`, 81722416bytes.
  - `lbug.h`: `7da5b7e36d6610e43a1512e4997ac123ba0886cf5598ae44aca5449aa02a8aa3`, 77406bytes.
  - `lbug.hpp`: `0b463bef0fa7a9ba1bb14cee12b23bf5bc7c2d6cfc4b3779d6f7cbe92bf4831d`, 333665bytes.
- Native `file` identifies a current ar archive; `lipo -info` identifies arm64. Full identities in `native-environment-identity.json`; direct-check outputs in `native-cache-direct-checks.txt`.
- Normal locked/offline Cargo metadata and dependency tree already resolve lbug0.19.0. The registry crate checksum also matches Cargo.lock. Its build script supports LBUG_LIBRARY_DIR + LBUG_INCLUDE_DIR as the direct external-native path, so no library acquisition/source compilation fallback is needed when both verified paths are supplied.

This validates existing local dependency bytes against official0.19.0 asset identity. It does not prove the failed hosted job's cached library bytes were the same, because that job did not print their hashes.

## Recommended future command environment — NOT APPLIED

Use from `/private/tmp/codebasegraph-pr163-ci-repair-20261030` with parent-exclusive build slot:

```text
LBUG_LIBRARY_DIR=/private/tmp/codebasegraph-copy153-evidence/native019
LBUG_INCLUDE_DIR=/private/tmp/codebasegraph-copy153-evidence/native019
LBUG_VERSION=0.19.0
OPENSSL_LIB_DIR=/opt/homebrew/Cellar/openssl@3/3.6.2/lib
MACOSX_DEPLOYMENT_TARGET=13.3
CARGO_BUILD_JOBS=2
```

Both existing local OpenSSL3.6.2 link dylibs are present, matching the prior local test environment. They are **developer test/build prerequisites only**, not an external-runtime consumer qualification concession. Native-artifact prepares its own pinned bundled OpenSSL. Deployment13.3 remains unchanged build metadata. Build-job2 is a resource recommendation, not a product constant; parent can assign resource concurrency. Keep LBUG_SHARED, CARGO_TARGET_DIR and inherited custom Rust flags unset. Do not reuse old project executables/target directories or change shared config/cache contents.

Default isolated `target/` is still absent. The first native Cargo build will compile project code and native FFI/grammar glue but use the validated prebuilt Ladybug library. Earlier COD54 build output can identify dependencies, not final-source executable provenance. The source root and manifest must appear in Cargo compiler-artifact/dependency records and the final normal CLI hash must be captured. No safe external target-cache executable reuse has been established.

## Runner/profile and planned baseline/candidate checks

`native-tests` inherits optimized release settings while keeping debug assertions, overflow checks and unwind, so deterministic lifecycle fixtures remain compiled. `.cargo/config.toml` sets serial test threads and forced HOME under this isolated target; record effective HOME. Use locked/offline execution to avoid acquisition. Exact new test names come from Backend; none are invented here.

- Bounded native test command after assignment:
  `cargo test --offline --locked --profile native-tests -p codebase-graph --lib <EXACT_FULL_TEST_NAME> -- --exact --test-threads=1`
- For genuine runner-written JUnit after coordinator assigns output/config ownership:
  `cargo nextest run --offline --locked --cargo-profile native-tests -p codebase-graph --lib -E 'test(=<EXACT_FULL_TEST_NAME>)' --test-threads 1`
  Confirm installed nextest filter/config syntax and discovered inventory before execution; JUnit config/output is not created in this environment-only pass. Do not synthesize reports or ingest into the older-root Scryer registration.
- Preserve test-only baseline (old250ms response bound) and candidate source hashes/diff. The deterministic delayed-START-ACK regression must reach the actual client request path and fail before repair/pass afterward; verify one accepted START/no blind replay, separate STATUS/recovery timeout behavior, malformed/unavailable control failure and fail-closed cancellation/cleanup. An error35 socket microprobe alone is not a project regression.
- Repository native gate after final candidate assignment remains the typed `cargo run -p xtask -- native-test --target macos-arm64 --cohort unit|generation|integration`, serial cohorts. Native artifact must use default isolated target because current packaging reads literal `target/release`.

**Next owner:** coordinator assigns exclusive baseline/candidate build slot and exact test/claim scope; Backend owns implementation/test edits; Evidence Verifier independently executes final checks after handoff. No heavy execution started here. Matching macOS26.6.2/25G83 host and validated libraries establish readiness, not current archive qualification or exact original-CI scheduling cause.
