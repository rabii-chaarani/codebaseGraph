# Independent public-release baseline — v4.0.4

**Result: VERIFIED FULLY PUBLISHED BASELINE for the two approved native targets.**

| Field | Evidence |
| --- | --- |
| Scope | COD-103 release-baseline audit only; `rabii-chaarani/codebaseGraph`. No implementation or publication action. |
| Selected source | `7125900c740297be5c60821dbeb9811f32a5894a`; tree `03e59639a6145280883253a9a21135abf60b373d`. |
| Canonical authority | [COD-103 spec rev0.2](https://github.com/rabii-chaarani/codebaseGraph/blob/7e79f7e924536f29d3212e83899b01c38afd39de/docs/specs/COD-103/spec.md), approved full SHA `7e79f7e924536f29d3212e83899b01c38afd39de`; [separate approval record](https://github.com/rabii-chaarani/codebaseGraph/blob/383a481bd5a71c58dab7dd796ace92a365760da5/docs/specs/COD-103/approvals/2026-10-29-combined.md), user decision 2026-10-29 “approve”. Both committed files independently read with `git show`. |
| Applicable criterion | Baseline reconciliation prerequisite for C103-R06/C103-A06 only, not final complete-queue release acceptance. No claims assigned. |
| Approved native targets | **macos-arm64, windows-x86_64**. Stale four-target wiki/skill wording does not expand this approved scope. |
| Public release | [v4.0.4](https://github.com/rabii-chaarani/codebaseGraph/releases/tag/v4.0.4), ID428697860; published `2026-10-30T05:56:11Z`; public, non-prerelease, immutable. Latest-release endpoint independently returns this release. |
| Source CI | [40997892360 / attempt1](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/40997892360/attempts/1): exact selected SHA, `.github/workflows/ci.yml`, `push`, `main`, success; complete 22/22 job inventory, all successful. |
| Governed publisher | [41010094891 / attempt1](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/41010094891/attempts/1): exact selected SHA, `.github/workflows/release.yml`, `workflow_run`, `main`, success. |
| Observation | 2026-10-30T23:45:12.487968+00:00. Role-map Evidence Verifier `inherit/inherit`; parent reports overrides omitted; actual model/effort metadata unexposed. |

## Criterion-level results

| Observation | Method and actual evidence | Result |
| --- | --- | --- |
| Exact public tag/source | GitHub `get_tag`: `refs/tags/v4.0.4` directly resolves to selected commit; release metadata agrees. | Verified |
| Complete approved assets | Exactly two named native archives plus their two SHA-256 sidecars; anonymous HTTP200 downloads of every actual public file. | Verified |
| Public bytes → CI source | Every public file matches both the original exact-source CI ZIP entry and publisher validated-set entry. ZIP digests themselves match GitHub artifact metadata. | Verified |
| Native provenance | Each original `provenance.json` schema1 names selected SHA, version4.0.4, its approved target/archive and the actual public archive digest. Internal checksums for both shipped binaries in both archives also match. | Verified |
| Packaged gate evidence | Source-CI native artifact jobs123891081922/macOS and123894546428/Windows each report successful `Build and smoke release artifact` and upload; package dry-run and `required` jobs succeeded. These are existing executed CI observations, not new local executions. | Observed successful |
| Actual registry crate | Normal public download API HTTP200, registry index checksum match, version/name/repository and embedded VCS SHA match below. | Verified |
| Finalization | Native validation/publish, crates publish and finalize jobs succeeded. Finalize job123917600667/attempt1 succeeded; PR170 is merged/closed with `autorelease: tagged`; public release is finalized/immutable. | Verified |

## Actual public asset bytes

| Actual download URL | Asset ID | Independently calculated SHA-256 | Bytes | Original CI + staged publisher |
| --- | --- | --- | ---: | --- |
| [codebase-graph-4.0.4-macos-arm64.tar.gz](https://github.com/rabii-chaarani/codebaseGraph/releases/download/v4.0.4/codebase-graph-4.0.4-macos-arm64.tar.gz) | 691107455 | `f879194913e1e9b92494483ca7a79626792465531f5fcacfea7b2ffe37a7a75a` | 14,386,632 | Match |
| [codebase-graph-4.0.4-macos-arm64.tar.gz.sha256](https://github.com/rabii-chaarani/codebaseGraph/releases/download/v4.0.4/codebase-graph-4.0.4-macos-arm64.tar.gz.sha256) | 691107454 | `c7544f00ac51506485dc2a626a83a1cbe8bb3cc67573b5cb7b3f1e99c970b553` | 106 | Match |
| [codebase-graph-4.0.4-windows-x86_64.tar.gz](https://github.com/rabii-chaarani/codebaseGraph/releases/download/v4.0.4/codebase-graph-4.0.4-windows-x86_64.tar.gz) | 691107451 | `b01d0645d05f17f5b0dc8ebe57292449ef63da2ae82ab816e61f2833c7918347` | 29,238,014 | Match |
| [codebase-graph-4.0.4-windows-x86_64.tar.gz.sha256](https://github.com/rabii-chaarani/codebaseGraph/releases/download/v4.0.4/codebase-graph-4.0.4-windows-x86_64.tar.gz.sha256) | 691107453 | `938bb6c02f1b9cf183fadc726adbb70743a1f1d6f1ef5aab4a0fec0ed603408a` | 109 | Match |

Each sidecar's named archive and hash match the downloaded archive. Both archives contain graph CLI, wiki CLI, internal checksums and the two installer scripts. Detailed inventories and binary checksum results are in `native-public-asset-inspections.json` and `native-internal-checksum-results.json`.

## Retained, independently downloaded artifact ZIPs

| Actual artifact download URL / ID | Independently calculated ZIP SHA-256 | GitHub digest |
| --- | --- | --- |
| [native-windows-x86_64 / 12957135488](https://api.github.com/repos/rabii-chaarani/codebaseGraph/actions/artifacts/12957135488/zip) | `32ea41fc0010c3131647fa17ff3e38eba864e21889aa239c4d2d88cc6c426010` | Match |
| [native-macos-arm64 / 12955489108](https://api.github.com/repos/rabii-chaarani/codebaseGraph/actions/artifacts/12955489108/zip) | `ed1b418516a87169edb1721b03833a5cf5186e57ab96341b48b6ec76ebaf0b57` | Match |
| [release-metadata / 12958080289](https://api.github.com/repos/rabii-chaarani/codebaseGraph/actions/artifacts/12958080289/zip) | `86c334e62ec89ecaab42bbfce1dcbd69bf4cf6013bbca259845249c466eefdce` | Match |
| [validated-release-assets / 12957666252](https://api.github.com/repos/rabii-chaarani/codebaseGraph/actions/artifacts/12957666252/zip) | `357cc46b1d6e6752e68a288f131beeec52aaa0872eb1d2de9875021b07937bf2` | Match |

Native artifacts originate from source CI40997892360/attempt1. Publisher artifacts originate from41010094891/attempt1. All are retained on disk here. Release metadata binds `source_sha=7125900c740297be5c60821dbeb9811f32a5894a`, `version=4.0.4`, `tag_name=v4.0.4`.

## Actual public crate

- Requested [normal registry download API](https://crates.io/api/v1/crates/codebase-graph/4.0.4/download); HTTP302 → HTTP200 at [actual registry file](https://static.crates.io/crates/codebase-graph/codebase-graph-4.0.4.crate).
- Actual compressed bytes: **8,458,919**; SHA-256 **`7cb5ff382352669cd1d0129c0a27ae2d4302d7edd8eb880cd6c20413557783ea`**.
- [Sparse registry index](https://index.crates.io/co/de/codebase-graph) independently supplies the identical4.0.4 checksum, `yanked=false`, publication timestamp `2026-10-30T05:57:42Z`.
- Actual archived `Cargo.toml`: `name=codebase-graph`, `version=4.0.4`, repository `https://github.com/rabii-chaarani/codebaseGraph`.
- Actual archived `.cargo_vcs_info.json`: `git.sha1=7125900c740297be5c60821dbeb9811f32a5894a`, `path_in_vcs=""`.
- Earlier version-API403 observations were not treated as absence and were not retried. The successful actual crate download and index comparison independently establish4.0.4 publication.

## Limits, preservation and next owner

- **Publication verification is complete for this baseline's two approved targets.** No four-target qualification is claimed. Stale wiki/procedure prose is a resolved authority distinction, not an unresolved target decision; no wiki edit was made.
- No test/build/install, binary or installer execution, CI rerun, fetch, Git/ref/settings/credential, repository, Scryer, Linear or memory write occurred. Only public reads/downloads and this session's diagnostic files were written. No constants changed.
- One narrow CI-gate log read was refused by `gh` because its response contained terminal escapes. No retry or whole-run log download occurred. This is not a permission-denial inference. Independent unique exact-source CI discovery and actual original/staged/public byte comparison establish the required chain without that log.
- This does **not** establish consumer-platform acceptance, Windows11 desktop/PowerShell qualification, later main/COD-54 qualification, complete COD-103 queue/candidate readiness, COD-110 implementation permission or aggregate closure. Parent's pending exact-root Scryer limitation remains unchanged.
- Old partial v4.0.3/run40851263747/attempt6 remains separate and untouched; no retry/upload/relabel/delete/retag or recovery was attempted.
- Reusable-memory assessment: transient baseline evidence; no memory write warranted or authorized.
- **Next owner:** Engineer may use v4.0.4 at the exact SHA above as the independently verified published baseline for authorized release-content reconciliation. Future gates and aggregate ownership are unchanged; this audit authorizes no publication.

## Durable evidence

`result.json` is the machine-readable result. It includes all actual URLs, run/attempt and artifact IDs, full hashes, provenance, per-check booleans, limits and exact authority references. Adjacent JSON files retain complete relevant API inventories and inspections; all downloaded archive/ZIP/crate files stay on disk. `authority-read-receipts.json` binds the local reads of the approved spec and approval record. `git-state.txt` records the unrelated shared working-tree state; this audit did not use that dirty tree as the release source.
