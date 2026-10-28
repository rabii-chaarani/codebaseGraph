# Isolated instruction walkthrough

Everything below is fictitious. Use the provided engineer skill and supplied authoritative rules to respond to the user request. Do not call real integrations, launch real workers, mutate a repository or create real tracking. Express tool/worker actions as mock requests and responses. All issue/spec/commit/source identifiers are synthetic. A mock result is not production evidence. Each variant starts independently unless marked as a continuation. Only the fixture input and its source files go to the participant; grading material is separate.

# Choosing work

All rules/bindings are verified for this mock checkout and workspace. Unless otherwise stated, committed approval, Scryer prerequisites and review-start gates have been checked against current authoritative records for every ready row; status labels alone are not that evidence.

## Variant A
User: “Recommend work in Parser / M2.” No issue has been selected. Lower Linear priority number means higher priority; milestone position is an existing order, not a score.

| Issue | Scope | Readiness evidence | Work unblocked | Priority | Milestone position |
|---|---|---|---:|---:|---:|
| CBG-MOCK-21 | Parser / M2 | approved; no unmet prerequisite | 2 | 2 | 4 |
| CBG-MOCK-22 | Parser / M2 | approved; no unmet prerequisite | 2 | 1 | 3 |
| CBG-MOCK-23 | Parser / M2 | approved; no unmet prerequisite | 1 | 1 | 1 |
| CBG-MOCK-24 | Parser / M2 | approved; no unmet prerequisite | 2 | 1 | 2 |
| CBG-MOCK-25 | Parser / M2 | Ready label; missing committed-spec approval | 9 | 1 | 1 |
| CBG-MOCK-26 | Parser / M2 | approved; unmet Scryer prerequisite P-contract | 8 | 1 | 1 |
| CBG-MOCK-27 | Search / M2 | approved; no unmet prerequisite | 20 | 1 | 1 |

## Variant B
User: “Find me something useful to implement.” No user-named scope exists. All scopes from Variant A are available.

## Variant C
User: “Pick candidates in Runtime / M3.” The only records there are CBG-MOCK-28 (requires unapproved public API revision) and CBG-MOCK-29 (approved spec but depends on unimplemented CBG-MOCK-28). Neither has implementation-ready prerequisites.

## Variant D
User: “Develop better parser cancellation in Parser / M2.” No issue named. Search by stable work terms returns archived CBG-MOCK-30, linked to canonical `docs/specs/MOCK-CANCEL/spec.md` revision 1 and user approval for cancellation restoration only. It was closed after a partial fix. Current request does not say whether it restores that behavior or adds a new contract. No new issue exists.
