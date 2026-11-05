# CBG-COPY-01 — Resource-ceiling removal amendment

## Decision and authority

Revision R1, 2026-10-26. Baseline: PR153 head `8e47d2b3c74b72b56a0b6b212d1bf939c8088ca7`. This additive amendment preserves the historical [Scope A spec](spec.md) and [approval](approval.md).

The user requested removal of the resource ceiling because it was an optimization rather than desired production behavior. After the assistant offered removal of hard budget enforcement and its options while retaining streaming/chunking, diagnostics and ownership/containment, the user selected: **“remove only the enforcement and options”**. Implementation and update of existing PR153 are authorized for this scope. No merge/deployment is authorized.

Scryer change `chg-vhbvd3` rewords existing `resp-263`, `resp-271`, `resp-273`, `resp-274` to remove configured-hard-memory promises while retaining failed-candidate abandonment, bounded execution window, deterministic spill/merge and staged search. Same-scope user sign-off recorded. No new topology or directives. Linear remains verification-pending.

## Requirements

| ID | Observable contract |
| --- | --- |
| CBG-COPY-01/RM1 | Remove `worker_memory_mib` and `rust_memory_mib` from supported CLI/config/API/MCP/native materialization requests, including defaults, validation, propagation and health claims. Do not add an “unlimited” replacement or an enormous sentinel limit. Existing stored fields follow ordinary unknown-field deserialization; no compatibility shim/migration. Old CLI flags are unsupported. |
| CBG-COPY-01/RM2 | Remove application RSS-triggered termination and artificial source/partition/artifact estimate rejection, plus the derived 128–384 MiB Ladybug pool policy. Use native-engine buffer-pool defaults instead of another application ceiling. Keep observation where useful, normal cancellation/owner-loss/unexpected-exit teardown, and fail-closed publication on real errors. |
| CBG-COPY-01/RM3 | Keep `spill_chunk_mib`, `max_parallelism` and single-thread controls as optimization choices. Continue sequential COPY, early indexes, external merge, spill and bounded queues. Chunk targets trigger flush/rotation, not admission failure: an individually large valid record can occupy its own chunk. Normal allocation/I/O errors and true encoding/format limits still apply. Do not restore whole-table buffering. |
| CBG-COPY-01/RM4 | Preserve Scope A: no same-candidate mutation replay, no existing-object error suppression, original error primary, bounded diagnostic ingestion/context and COPY-query versus explicit-checkpoint distinction; preserve prior published graph on failed candidate. |
| CBG-COPY-01/RM5 | Preserve containment/ownership/leases, cancellation, unknown-teardown evidence, corruption/path/format checks, protocol/framing/diagnostic bounds, read-format validation and bounded disk-cleanup pass policy. No retention/platform policy redesign or dependency upgrade. |

Natural OS/native OOM and full-disk errors remain possible; this is removal of application workload enforcement, not a guarantee of unlimited physical resources. The public worker/Rust budget knobs are removed, while batching/concurrency tuning remains. Automatic candidate reconstruction remains excluded.

## Implementation sequence and ownership

1. Public propagation: remove worker/Rust fields from request/config types, CLI/help/MCP schemas, refresh/daemon options and health output; update real callers and fixtures rather than leaving dead fields. Shared contracts have one writer.
2. Runtime: remove RSS-kill policy from worker/native/guardian paths and budget estimates from source, partition and artifact load. Use native pool defaults. Retain metrics when practical and exact cleanup/cancellation ownership.
3. Streaming: keep spill/merge/file rotation but flush an oversized valid record into its own run/chunk. Keep security/corruption/read-format and maintenance-pass limits; do not create a general resource framework or a fake maximum.
4. Verify: replace obsolete budget-rejection tests with no-ceiling behavior, retain real CLI COPY/reopen/PK/connector/search and Managed/Direct preservation, and rerun ownership/cancellation tests. Update curated wiki through k-wiki. Require independent proposal and final review with GPT-6.1 Sol/Max; backend workers GPT-6 Luna/High. Detailed briefs/prerequisites/file ownership live in Scryer.

## Acceptance

| ID | Testable pass condition |
| --- | --- |
| CBG-COPY-01/RA1 → RM1 | CLI parsing/help, config, typed requests and MCP schema expose no worker/Rust memory option. Old stored fields cannot continue enforcing a limit; ordinary builds need no replacement ceiling. Test actual entrypoints, not magic-string-only checks. |
| CBG-COPY-01/RA2 → RM2,RM5 | Controlled fixtures exceeding a deliberately small former budget are not rejected by application estimates or RSS policy. No live-system exhaustion experiment. Source/partition/artifact and worker/phase paths contain no artificial budget admission/kill; cancellation/owner-loss still stop owned work and preserve evidence. |
| CBG-COPY-01/RA3 → RM3 | Small spill targets still rotate; an oversized valid JSON/CSV/spill/search record succeeds as an individual chunk rather than failing solely for target size. Logical rows, IDs, JSON, endpoints and search match a control after separate-process reopen. No whole-table fallback. |
| CBG-COPY-01/RA4 → RM4,RM5 | Existing real COPY acknowledgement-failure, early-hash/chunk/reopen, scan-versus-PK, connector/search, Managed/Direct prior-bundle and independent rebuild tests pass. Existing unknown-containment/cancellation/owner-loss behavior remains; no retry or unsafe cleanup is added. |
| CBG-COPY-01/RA5 | Formatting/strict Clippy, affected/full suites, native-test profile and ordinary macOS ARM64/Windows x86_64 CI pass or are explicitly pending. Keep exact source/binary/native provenance and failed attempts. No hidden replacement ceiling, production fallback for tests, weakened unrelated assertions or silent platform expansion. |

## Risks and qualification

Removing enforcement can increase memory/disk use and expose real OS/native allocation or I/O failure. The user selected that behavior. Spill targets are not maximum accepted record sizes; test framing/rotation and merge behavior to avoid infinite rotation or an unbounded batch. Public-field removal touches many consumers, so compile all targets/features and inspect serialized schemas. Cleanup's bounded per-pass budget and security/protocol framing remain outside this workload-ceiling removal.

Earlier long-running tests use frozen pre-amendment binaries; their eventual results are labelled as such and do not qualify this changed source. No R1 implementation edits exist at creation. Commit this amendment and bind the explicit user approval before dependent implementation; independent review and actual qualification remain separate gates.
