# CBG-IDENTITY-01 preparation evidence

2026-11-16, Australia/Darwin. Scope at this checkpoint: contracts and unsigned architecture preparation only.

## Verified observations

- Main/base `90e2ab6fae2767bdb158e19f0072d43568e80dbc`; isolated `/private/tmp/codebasegraph-identity-20261116`, branch `codex/fix-managed-identity`. Existing canonical knowledge edits and COD-113's unfinished work remain untouched.
- Installed Cargo metadata identifies `codebase-graph 4.2.0`, release, aarch64-apple-darwin, from the canonical repository path. That is not exact source/binary attestation; freeze/check the actual executable and dependency closure before runtime tests/upgrades.
- Valid CGGGATE1 binding, version1, SHA-256 payload checksum passes; reserved bytes zero. Binding SHA256 `5196a57190cb555540c7a3b382e3b8ec47187bb953c535a7713b700baddbc5cd`.
- Managed root and seven checked bound objects have recorded device16777232/current16777235 with matching inodes. Root inode85208939; no inode-only identity rewrite was performed.
- Binding creation2026-11-07 19:16Darwin; selected publication2026-11-13 14:58Darwin; kernel boot2026-11-16 08:46Darwin. Renumbering across reboot is consistent with observations, not proof of an unstored historical UUID.
- Normal full build, native-access retry, quiesced retry and normal daemon initialization all return generation_gate_binding_changed. The daemon is stopped; no fresh graph was published. Configuration SHA256 `c13025ca36ca1e7937ba9e19888a2d1dc5dfba2f2e181a2665a623e0fc662f1f` and binding bytes remained unchanged during investigation.
- Ordinary graph MCP is unavailable and CLI is blocked. Source targets came from the retained graph manifest file index; per-file hashes matched all inspected indexed Rust files. Narrow audit found27 FileIdentity-related modules and additional raw Unix FIFO device/inode fields. This is source/metadata inspection, not a native graph-guard bypass or a fresh semantic dependency graph.
- Read-only ctypes fgetattrlist/ATTR_VOL_UUID on the held store directory returned success,20bytes/nonzero UUID, with native identity unchanged. Kernel boot-session UUID is available under normal native permissions; the agent sandbox denied that sysctl until read access was escalated. No UUID value is needed in this report.
- COD-113 is now Canceled by explicit maintainer direction. Its WIP, models, approvals, diagnostics and failure evidence are preserved; cancellation is not quiescence or successful repair.

## Approval and qualification status

The user's selected plan and implementation request authorize contract/authority preparation. Exact committed-spec/model approval remains required before dependent source code. Actual preview-fingerprint approval remains required before live enrollment. No approval record is pre-created.

No product source change, Rust build, new native qualification, passing identity-recovery test, current-store enrollment or implemented model verdict exists at this checkpoint. Independent static verification/review and immutable proposal references will be recorded after they genuinely exist. Other issue-specific qualification waivers are not inherited.

## Independent static checks

Frozen initial proposal `943884fe915f16be876c483ab077be31b4935056`, base above: independent Terra/medium verification passed exact three-file scope, JSON, diff/whitespace, all24 requirement IDs covered by10 criteria, two resolving local links, and four scoped entries matching live Scryer id/kind/owner/change/delta. It did not independently certify historical observations, approvals, source/runtime behavior or the42-entry preservation assertion.

A fresh default/fork-none Sol/xhigh reviewer used adversarial-review-engineer, read the complete artifacts and found no confirmed P0-P3 document defect. It screened identity/admission, ownership, mixed versions, crash/cancellation, certainty, Direct compatibility and traceability. Actual fence refusal, persistent descriptor/boot behavior and fault tests remain future qualification. It identified an approval-readiness limitation: added claim IDs lacked statement text in the exported delta. The corrected export includes full statements/concerns read from their live owning nodes.

Focused Final Review and independent Terra/medium verification passed at `6793034af1ca9a3cf3124d333f264f209c39017b`. The verifier matched four pending entries and all three statements/concerns to live Scryer owners; JSON, links, 24 requirement/10 criterion coverage and diff checks passed. The reviewer found no confirmed P0-P3 in the semantic-export delta. Specification SHA256 is `1352a8108a283b6070482a82b75387d90587744249827dce4fae0ccd01dac132`; architecture snapshot SHA256 is `1180e75d64c2538a1c77ce40b44fbd4af138126f3fa343ddf2abd6adb90e66ab`. These are static proposal results, not product/runtime/qualification results or exact model approval. This factual result update changes neither contract nor model semantics.

## Unsigned architecture preparation

Scryer change `chg-r9y7bh` holds the three genuine responsibilities and its same-change execution brief in `node-110.description`, captured in [architecture-proposal.json](architecture-proposal.json). Server-assigned claim IDs are `resp-y5d311`, `resp-xpv8tb` and `resp-1e1m51`. The brief reserves the exact source/test set and one future complete owner; activation remains gated by committed-spec/model approval and verified quiescence. Unlisted files/manifests require coordinator reassignment before edit; no product writer is active.

Automatic approval review rejected a proposed second Scryer change because this work already had `chg-r9y7bh`, citing duplicate/fragmented authority risk. The second change was not created. The safer alternative keeps the detailed execution record inside the existing change; it does not refile other work or create another authority. The detailed description produces one new nonblocking length advisory in addition to the three pre-existing description advisories; disclose it as an intentional execution-record formatting limitation, not a passing zero-warning claim. No implemented verdict or approval is inferred.
