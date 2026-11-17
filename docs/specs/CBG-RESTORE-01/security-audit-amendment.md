# CBG-RESTORE-01 — Minimal sanitizer audit-gate amendment

Revision SEC-0.1, 17 November 2026 (Australia/Darwin). Work: CBG-RESTORE-01 / COD-136, REST-R03/R11 and REST-A06/A10/A12. **Proposed only: no new dependency-update approval is recorded.**

## Executed blocker and primary evidence

The frozen c5 candidate's [CI run 44038059156](https://github.com/rabii-chaarani/codebaseGraph/actions/runs/44038059156), cargo-audit job 133885650401, failed because Cargo.lock selects ammonia 4.1.8 and the fresh advisory database reports [RUSTSEC-2026-0533](https://rustsec.org/advisories/RUSTSEC-2026-0533.html), reported 12 November 2026. This is an existing dependency audit blocker, separate from the confirmed restoration-code regressions.

The [primary RustSec record](https://github.com/RustSec/advisory-db/blob/main/crates/ammonia/RUSTSEC-2026-0533.md) identifies patched 4.1 versions at 4.1.9 and above below 4.2.0. The [upstream 4.1.9 release](https://github.com/rust-ammonia/ammonia/releases/tag/v4.1.9) and its Cargo.toml confirm a patch on the existing minor line, unchanged dependency families and Rust minimum 1.80. This does not establish application-level exploitability: the advisory's triggering tag configuration must be distinguished from default sanitizer behavior.

The approved restoration scope retained merged security patches and excluded unrelated dependency upgrades. A concrete new audit-gate exception is therefore requested instead of changing dependencies or adding an advisory ignore under inferred permission. Existing runtime/CI approvals remain valid for independent work.

## Exact proposed update

Recommendation: update only the existing sanitizer dependency to its smallest patched 4.1 version.

| ID | Required change / constraint |
| --- | --- |
| SEC-R01 | In crates/k-wiki/Cargo.toml, change ammonia from ~4.1.8 to ~4.1.9. Retain the existing minor-line bound, features and all sanitizer configuration/public interfaces. |
| SEC-R02 | Resolve Cargo.lock to ammonia 4.1.9, registry checksum 935c841d86ee68ba89a983ea6e4255436f769ab1e2b701574653baf4b30739ce. All other package versions, checksums and dependency edges remain unchanged. Unexpected resolver drift stops for review; no broad cargo update. |
| SEC-R03 | Preserve Ladybug 0.19 SDK/ABI, native assets/runtime policies, root Rust1.98/explicit1.98.1 builds and other members' declared minima. No new crate, feature, HTML allowlist, rendering API or backend change. |
| SEC-R04 | Keep the existing audit policy, including its established time-parsing precondition and existing warning handling. Add no new ignore, advisory exemption or weakened security/CI gate. |
| SEC-R05 | Execute relevant wiki rendering/sanitization, API/transport and package regressions plus strict Clippy and the unchanged audit command on the final dependency/source snapshot. Bind hosted native and required results to the final PR head/base; no release or live activation. |

An inert copy of c5 at /private/tmp/cbg-restore-audit-proposal-20261117 was resolved with cargo +1.98.1 update -p ammonia --precise 4.1.9. The initial offline resolution was unavailable and is recorded; the subsequent normal registry resolution succeeded. Comparison shows only ammonia version/checksum changes in Cargo.lock, with no transitive update. Proposed lock SHA256: 11150d59c7c32e7a6b8848bf14b7370ae50767003c19a6c5ea2e7d6c7918e6e8. Proposed wiki manifest SHA256: 04407d46b510f12266c9d0841f4048c6fbffa6e06803a8bebe9073c11c012fa9. Neither product manifest nor lockfile was edited by this preparation.

## Ownership, acceptance and recovery

The same sole technical owner /root/restoration_implementation retains the two already-owned manifest/lock paths after actual exact-revision approval and bounded root GO. Root owns this proposal/decision, tracking and serialized Git. No additional product source file is assigned by this amendment.

- SEC-A01: the manifest lower bound and resolved package equal SEC-R01/R02; a structured lock comparison proves every other package version/checksum/edge unchanged.
- SEC-A02: the unchanged audit command no longer reports this blocking advisory; existing precondition/warnings are recorded accurately, not misrepresented as an exhaustive security scan.
- SEC-A03: relevant wiki and packaging checks pass on the patched dependency with the current runtime fixes, and required final-source native/independent-review/publication gates remain satisfied. Configured jobs or earlier green sources do not qualify this snapshot.

If the patch fails compatibility, resolution or audit, retain the failure and keep qualification pending. Do not silently select 4.2, alter tags, waive the advisory or publish a replacement package. Restoration changes remain reviewable; abandoning this proposed exception requires no rollback of installed software because none is changed.

## Human decision pending

Approve this exact committed minimal audit-gate exception, or keep the original dependency and leave the required audit gate unresolved. A preselected answer, the earlier restoration/CI approvals, reviewer result or proposal commit is not approval of this new dependency change.

The actual decision record must later cite this path, full Git SHA, exact scope and question/answer reference. This file does not embed its own containing SHA. Original approvals d809fe0f8289bbf7d167a891bd7c31160f27f0e1 and d091767dd26d5b25f963d20129bd0abc982fc46b remain unchanged; no merge, release, live store/runtime/client/service change or unrelated issue closure is included.
