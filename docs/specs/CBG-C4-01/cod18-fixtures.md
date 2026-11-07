# COD-18 planned trust and integration cases

## Status and harness boundary

Work: `CBG-C4-01/F01/S04` / [COD-18](https://linear.app/codebasegraph/issue/COD-18). These are **planned fixture definitions**, not executed product tests or hosted qualification. They instantiate [contract revision 0.1](cod18-contracts.md), supplement the inherited [COD-16 cases](cod16-fixtures.md) and preserve their historical identities.

`H`, `B`, `S`, `T`, `C`, `E`, `I` and named users below are fixture metavariables. Substitute complete C16 GitRevision/digest/state/evaluation identities and independently established platform facts in the eventual harness. No placeholder or candidate flag is runtime evidence. Small metric values are illustrative arithmetic, not numerical budgets or execution caps.

Use a controlled fixture repository and explicit authorization for hosted mutations. Preserve source/configuration/tool revisions, exact attempted path, current platform facts, result and actual accepted/integration identity. Demonstrate rejection at the real protected boundary, including an attempted alternate writer; asserting a local helper returned false is insufficient for A18. Never perform production mutation probes as part of this documentation work.

## Authorization and bootstrap

| Case | Given / mutation | Required observation | Trace |
| --- | --- | --- | --- |
| C18-F01 | No maintainer-established bootstrap anchor; valid-looking candidate/table | Acceptance blocked; candidate cannot establish trust itself | R10/A13; C18-02 |
| C18-F02 | Missing verifier/configuration pin or unavailable approved artifact | No implicit latest/default verifier; activation or dependent check blocked/UNKNOWN | R10/A13; C18-02 |
| C18-F03 | Candidate names a different verifier, reviewer or gate | Reject/ignore as authority according to strict input schema; never activate it | R10/A13; C18-02 |
| C18-F04 | Candidate declares sole-maintainer mode while active trust uses designated_review | No self-approval; active trusted mode controls | D11/R10/A13; C18-01 |
| C18-F05 | Sole-maintainer trust config lists two active human maintainers | Invalid configuration; no automatic per-author exception | D11/R10/A13; C18-01 |
| C18-F06 | Manual action from another user/bot or merely a current write/admin user | Not the designated human; approval unqualified | R10/A13; C18-04 |
| C18-F07 | Correct human action for H, but proposal head differs | Invalidate subject; require new explicit approval | R10/A13/A18; C18-04 |
| C18-F08 | Root unchanged; one referenced constraint/waiver file changes or root references change | Independently recomputed governing closure differs; old approval cannot qualify | R10/A13; C18-03 |
| C18-F09 | designated_review mode; approving record identifies proposal author | Reject as nonqualifying, even if an API stub fabricates APPROVED | D11/R10/A13; C18-03 |
| C18-F10 | Older review APPROVED; later effective disqualifying state/dismissal | Do not select historical approval around the current state; missing ordering facts UNKNOWN | R10/A13/A18; C18-03 |
| C18-F11 | New explicit sole-maintainer dispatch; independently authenticated identity/control and exact subject | Human-authorization predicate may qualify; other mandatory checks remain required | D11/R10/A13; C18-04 |
| C18-F12 | Automatic PR/push/schedule/workflow-run event with identical approval-looking inputs | No sole-maintainer approval; no automatic action is promoted into a human decision | R10/A13/A18; C18-04 |
| C18-F13 | Re-run an originally human-triggered workflow; original actor still matches | No fresh approval; require new explicit dispatch rather than inherited actor privileges | R10/A13/A18; C18-04 |
| C18-F14 | Reuse an action/run reference for a different attempt or terminal run | Reject reused authority; explicit retry establishes a new subject/action | R10/A13/A18; C18-04 |
| C18-F15 | Deterministic results exist before qualifying approval arrives | New final authorization attempt; retain only still-valid deterministic evidence | R09/R10/A12/A18; C18-03/07 |
| C18-F16 | Withdraw/cancel/disqualify the relied-on action before the transition point | The protected transition must not occur; if platform race cannot be excluded, qualification stays blocked | R10/A18; C18-04/06 |
| C18-F17 | Transition commits, then cancellation/receipt loss occurs | Discover/report committed identity; no claim of rollback or blind repeated promotion | R02/R09/A02/A17; C18-06/07 |

## Trust changes, producer identity and failed work

| Case | Given / mutation | Required observation | Trace |
| --- | --- | --- | --- |
| C18-F18 | Active trust/configuration changes during an attempt | Authorization invalidated; no promotion under stale pins | R09/R10/A12/A18; C18-02/06 |
| C18-F19 | Candidate changes trust and uses its proposed human/verifier to approve itself | Same-candidate activation rejected; active old trust remains authoritative | R10/A13; C18-02 |
| C18-F20 | Separate trust update authorized under T with expected T; active trust already T2 | Conflict; preserve T2, re-evaluate under current authority | R02/R10/A02/A13; C18-02 |
| C18-F21 | Effective execution policy changes while source bytes match | Dependent input identity changes; regenerate/revalidate; no permissive resource defaults | R09/R12/A12/A15; C16-09/C18-02 |
| C18-F22 | Matching check name/application ID, wrong workflow/source/configuration | Reject producer/path; application binding alone cannot qualify | R10/A18; C18-05 |
| C18-F23 | Trusted job attempts to load candidate action/build script/verifier with approval/integration credentials | Reject that execution design; qualification cannot treat candidate control as trusted verification | R09/R10/A12/A18; C18-05 |
| C18-F24 | Manual-dispatch job is green but does not satisfy/bind the actual protected integration gate | No acceptance qualification; demonstrate the real protected-path connection separately | R10/A18/APR-04; C18-05/08 |
| C18-F25 | Each mandatory upstream provider is failed, skipped, cancelled, neutral, missing or incomplete in turn | No acceptance in every variant; unfinished evidence UNKNOWN, known independent violations retained | R09/R10/A18; C18-06/07 |
| C18-F26 | Waiver valid at first green; expires before final transition, including expiry equality | No waived PASS; raw overrun and any provable ordinary violation remain visible | R07/R10/A10/A18; C18-06/07 |
| C18-F27 | API/clock/review/run data unavailable, rate limited, partially paged or contradictory | Dependent mandatory predicates UNKNOWN; no incomplete-success inventory | R09/R10/A13/A18; C18-07 |

## State, composition, integrity and recovery

| Case | Given / mutation | Required observation | Trace |
| --- | --- | --- | --- |
| C18-F28 | Candidates 8 and 9 expect accepted 10; 8 is accepted first | 9 conflicts, re-compares with 8 and cannot surrender the improvement | R02/R07/A02/A10/APR-03; C18-06 |
| C18-F29 | Proposal one adds A→B, proposal two adds B→A; independent heads acyclic | Actual combined graph exposes a cycle and blocks the applicable mandatory check | R02/R06/A09/A17/APR-03; C18-06 |
| C18-F30 | Target base moves after action/evaluation, even if candidate head is unchanged | Exact base/subject mismatch; fresh evaluation and manual action required in sole mode | R09/R10/A12/A18; C18-04/06 |
| C18-F31 | Claimed table changes a result/value, omits a mandatory row or substitutes execution reference | Independent enumeration/regeneration detects mismatch; no trusted result from matching input hashes alone | R09/R11/A12/A14; C16-09/C18-06 |
| C18-F32 | New report-only commit has equivalent permitted inputs/mapping facts | Deterministic reuse can qualify with original provenance; old manual approval does not transfer to the new head/attempt | R09/R10/A12/A18; C18-03/04 |
| C18-F33 | Local caller supplies the ID of a real trusted workflow run | Locator alone grants no authenticated execution capability or promotion authority | R10/A13/A18; C18-04/05 |
| C18-F34 | Disconnect after possible dispatch; actual work may still be running | Hold actual execution accounting/leases; inspect current state; no automatic replay or rollback | R02/R12/A02/A16/A17; C16-09/C18-07 |
| C18-F35 | Attempt integration through ordinary merge, direct push, alternate workflow/API or local accept | Protected-path qualification must show rejection of acceptance-capable bypasses; otherwise remain unsupported | R10/A17/A18/APR-04; C18-05/08 |
| C18-F36 | Available platform cannot atomically cover freshness/expected state with integration | `unsupported_integration_path`/UNKNOWN and blocked acceptance; do not weaken the contract | R10/A18/APR-03/APR-04; C18-06/08 |
| C18-F37 | Two new manual attempts name the same expected state | At most one transition; the other conflicts; receipt loss does not justify replay | R02/R10/A02/A17/A18; C18-04/06 |
| C18-F38 | Expired waiver is renewed/expanded but retains its old approval reference | Changed constraint closure requires fresh exact approval; old approval cannot extend expiry or scope | R07/R10/A10/A13; C18-03/06 |
| C18-F39 | Correct actor string but no authenticated explicit decision or candidate can use approver credentials | No established human-authorization boundary; qualification blocked/UNKNOWN | R10/A13/A18; C18-04/05 |
| C18-F40 | Verifier's source revision matches but artifact/dependency/configuration digest differs | Reject unapproved executable control closure; no source-name-only trust | R09/R10/A12/A18; C18-02/05 |

## Evidence and downstream ownership

These 40 definitions do not execute the extension, approve a native model, establish Q1 coverage or qualify a GitHub path. Static documentation checks may count/trace cases and verify the synthetic graph arithmetic only. COD-37/46/47 and their prerequisites must turn them into owned, executed tests and controlled hosted evidence after COD-19 approval.

Retain executed evidence at canonical linked locations or in the agreed summary-table records as applicable. Bind tested/reviewed source, active trust/configuration, producer/run/attempt and actual integration identities. Loss of required live facts must produce a blocked/UNKNOWN decision; a copied historical receipt is not current authority. Preserve C16's no-replay, resource policy and one-table contracts.
