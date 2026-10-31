# D05a amendment — current macOS qualification only

- Work: `CBG-PLAT-01/D05a`; affects **S03/COD-54, R03/A05–A06**, and **S10/COD-61, R10/A15**.
- User decision: **“well then just drop it and keep only current”**, this Prime Agent conversation, 2026-10-27. The immediately preceding discussion explained that standard hosted CI uses current macOS but does not provide a macOS13.3 runner, and proposed a separate minimum-version runner. The user explicitly dropped that minimum-version qualification.
- This is the user's replacement policy, not an inferred passing test. It supersedes only the minimum/current macOS scope in the [original D05a decision](d05a-macos-supported-minimum.md). That decision and all earlier spec/approval revisions remain historical records; none is edited, deleted or silently advanced.

## Replacement policy

Qualify **only the current stable macOS release on Apple Silicon (ARM64)** at verification time. Remove the macOS13.3 execution requirement and any claim that older macOS releases are supported by this work. Record the actual tested macOS version/build, source revision and final installed archive digest. The `macos-latest` label alone is not OS-version evidence; inspect the job's actual OS and ensure it represents the current release.

Run the final installed archive's existing CLI, graph/JSON/FTS, reopen/search, wiki/MCP and failure checks on that current environment. Hosted CI may provide the execution evidence when it runs the actual current macOS release and demonstrates that the bundled runtime resolves without separately installing or relying on Homebrew/OpenSSL. Preserve D02's bundled compatible dylibs, relative paths, licenses, checksums, preflight failures and exact-artifact provenance. This decision does not waive those runtime/clean-environment requirements or turn an unexecuted/configured job into passing evidence.

The existing **13.3 deployment target and Mach-O compatibility audit remain unchanged build metadata**, not a support or execution guarantee for13.3. Removing the old qualification gate does not require raising link targets, changing OpenSSL/Ladybug/extension versions, rebuilding a different artifact recipe, or restoring retired platforms. The user did not request those changes.

## Scope and approval history

Revision0.11 records this narrow policy/acceptance amendment. A separate approval record pins its full committed SHA and this exact decision before dependent delivery changes. Existing S03 implementation authorization remains in force for unchanged R03 behavior; only the qualification scope is replaced. S00/COD-51 records this amendment, not approval of other implementation slices or global closure.

No Windows baseline/PowerShell/source-install policy, S09 disposition, other issue, signing/notarization policy, merge, publication, deployment, restart or aggregate closure is changed. Existing maintainer closure ownership remains. Prior executed evidence keeps its original source/environment identity and limitations; a historical "13.3 qualification pending" statement is not rewritten as if it had passed or had never been required.

## Linked-issue impact

| Record | Effect |
|---|---|
| COD-51 / S00 | Record exact replacement-policy revision/SHA and user decision; preserve earlier approvals. |
| COD-54 / S03, R03/A05–A06 | Remove only13.3 execution/support gate; retain current-macOS final-archive runtime/failure evidence. |
| COD-61 / S10, R10/A15 | Same current-only macOS qualification scope; Windows and aggregate gates unchanged. |
| Maintenance CBG-PLAT-01 milestone | Current-only macOS scope; no automatic issue/milestone closure. |
| PR155 / canonical spec | Publish revision0.11 and immutable amendment approval. |
| PR163 / implementation evidence | Link new scope while retaining reviewed/tested product SHA and historical reports. No product/workflow change required. |
