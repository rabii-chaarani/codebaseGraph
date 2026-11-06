# COD-16 documentation review and evidence

## Identity and limits

- Work: `CBG-C4-01/F01/S02` / [COD-16](https://linear.app/codebasegraph/issue/COD-16/cbg-c4-01f01s02-concrete-ontology-yaml-evidence-and-public-operation).
- Date/environment: 6 November 2026, Australia/Darwin; Darwin arm64 documentation worktree.
- Documentation baseline: `f67e1d95862484dee34c8e818c98b72959366c5b`, COD-17 PR #184 head; target dev baseline `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9`.
- Examined product source baseline: clean `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, version 4.2.1. No product source is changed by COD-16.
- Documents: [supplement revision 0.2](cod16-contracts.md), [decisions](cod16-decisions.md), [illustrative fixtures](cod16-fixtures.md), and this review record.
- Scope: documentation finalization; no product implementation, executed extension suite, supported-platform qualification, live protected-path change, authority migration, merge, release or issue closure.

## Earlier planning review

A fresh read-only reviewer used `default` / `gpt-6.1-sol` / `xhigh` / `fork_turns: none` and [adversarial-review-engineer](/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md). Current user AGENTS assignment settings override the skill's Max default. The original temporary draft hash was `9ccebff3a98173ab83b1362332f16e445ab9d05d9aeb6d7da931bb2a1038cb7e`; review included explicit decision/refinement overlays supplied in the chat, rather than a persisted final diff.

Returned outcome: no confirmed P0–P3 contradictions in the effective proposal. Four contract questions were identified and resolved through refinements: export/path-only ownership predicates; independent coverage/lineage types; tagged scenarios with explicit paired reach and exact contract-set comparison; source provenance/content-equivalence versus live authorization. Focused verification closed all four. This planning review does not qualify the newly persisted diff.

## Executed documentation checks

Executed with Python 3 in the Darwin arm64 worktree:

```text
python3 /private/tmp/codebasegraph-cod16-contracts-20261106/validate_docs.py /private/tmp/codebasegraph-cod16-finalization-20261106
```

Validator SHA-256: `c17ded4bc72466ec5f2be6f62d00e4e3ef838311e26c09abc85ec11c5b33bcb9`. The temporary script is retained at that path; this report retains its methods/results so its continued availability is not product evidence. It uses Git blob comparison for all five historical files, changed-path confinement to the four new COD-16 documents, `git diff --no-index --check` per new document, local Markdown target existence, fenced JSON parsing, contract/decision/fixture ID checks and selected synthetic graph calculations.

| Executed check | Result and limit |
| --- | --- |
| Historical byte identity / diff confinement | PASS: all five inherited files equal baseline blobs; only four new COD-16 documents permitted |
| Whitespace and fenced examples | PASS: no whitespace errors or unmatched fences |
| Local Markdown links / JSON syntax | PASS: 15 local links and two JSON fixture blocks at initial execution; remote availability checked separately at delivery |
| Contract/decision/fixture trace | PASS: C16-01–09, six choices, requirement/review identifiers and F01–F48 definitions present; identifier coverage is not semantic proof |
| Fixed resource caps | PASS: stale proposed numeric caps/default constructor wording removed; configured values required |
| Synthetic graph data | PASS: mixed-owner deduplication/lifting, mediated/bypass/disconnected path examples and F33 SCC/reach arithmetic match the documented expectations |

The first static trace check flagged compressed APR-03–04 wording; the traceability row was expanded to explicit APR-03/APR-04 and the complete check passed. This was a documentation-check correction, not a product regression. Fixture definitions remain planned product checks; parsing/calculating their documentation is not execution of a compiler, resolver, evaluator or acceptance suite.

No Rust build or product test is necessary for this documentation-only change and none is claimed. Existing baseline/CI evidence in COD-17 qualifies existing behaviour only.

## Independent persisted-diff review

Pending the required fresh read-only Final Reviewer with `default` / `gpt-6.1-sol` / `xhigh` / `fork_turns: none`, using adversarial-review-engineer against the documentation baseline and actual final content/fixtures/evidence. Confirmed P0–P3 defects must be remedied and affected paths reverified before PR handoff. This section will retain exact reviewed hashes/snapshot, outcomes and limitations.

## Delivery and retained gates

PR publication evidence pending. PR #184 remains a separate open prerequisite; no merge is requested. Current repository workflow branch filters do not include dev PRs, so no hosted CI result is implied by this documentation delivery. Record actual run/check availability after publication; do not label absence as green.

COD-18 trust/integration, COD-19 full-SHA product implementation approval/procedure transition, COD-25/F04 Q1 sufficiency, COD-40 scenario instances, D17 calibration and maintainer aggregate closure remain pending with their owners. Scryer `chg-7jc6yv` is proportional documentation tracking with no claims/links/folds. Assess reusable knowledge at handoff; issue-specific decisions are retained in the specification documents without duplicating them into memory/wiki.
