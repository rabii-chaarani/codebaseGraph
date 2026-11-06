# COD-17: Independent documentation review

## Review identity

- Date: 6 November 2026, Australia/Darwin.
- Work: `CBG-C4-01/F01/S03` / COD-17; documentation handoff, not implementation or platform qualification.
- Reviewer: fresh read-only `/root/cod17_final_review`, `agent_type: default`, `model: gpt-6.1-sol`, `reasoning_effort: xhigh`, `fork_turns: none`.
- Method: [adversarial-review-engineer](/Users/rabii/.codex/skills/adversarial-review-engineer/SKILL.md). The user's explicit current AGENTS.md assignment settings take precedence over the skill's Max default.
- PR base: `2f1fe10f6fc5f275f61e4a9bc2d38b4b33a7b5a9` (dev).
- Examined source: `90e2ab6fae2767bdb158e19f0072d43568e80dbc`, clean canonical checkout.
- Reviewed restoration commit: `0a66cb31e397e523190618f211a613f7a095d9a0`, plus the frozen new baseline document listed below.

| Reviewed document | SHA-256 |
| --- | --- |
| [Historical specification](spec.md) | `f8161559e5183985d62c41b0157225bb643901a365c35098c6b1aa6b08d69b1e` |
| [Historical proposal review](proposal-review.md) | `2f9f6de504f31d3b171239c00cd7837c576605e3e794fa96acd76b800cc4f1fd` |
| [COD-17 baseline and boundary proposal](cod17-baseline.md) | `9fc7ab3c651dfb43168b741559cda8757988b96b0112883dc5008aa8e4af4177` |

This record preserves the returned verdict. It does not extend that review to later changed content, select an architecture alternative or constitute user approval.

## Returned verdict

No confirmed P0–P3 defects found in the reviewed documentation snapshot. The proposal can proceed to documentation handoff while retaining its stated user-decision and qualification gates.

The reviewer confirmed the consequential claims through targeted document, source and Scryer checks:

- The exhaustive public request enum, public struct fields, executor obligation and private handler coupling support the compatibility warning. C1 and C2 remain proposed alternatives; C2 requires migration and renewed approval.
- Q1 discloses unsupported Rust cases and requires actual pilot sufficiency before calibration. Source confirms reverse name-index selection without caller-scope qualification. The observed placeholder gap remains distinct from unexecuted wrong-concrete-target controls.
- Existing Graph Read Service and Language Syntax Graph responsibility boundaries support the stated placement. Native Architecture remains proposed.
- Native-generation verification retains private token construction and genuine session/read borrows; sealing rechecks writer and current-generation identity. Both alternatives must preserve those restrictions.
- Historical recovery is contextualised without silently revising D01–D23, R01–R12 or APR-01–APR-05. Existing tests and hosted CI qualify existing behaviour only.

No corrective changes were requested. The three reviewed input hashes were checked again at review completion and remain unchanged in the delivered package.

## Open decisions and limits

The C1 dispatch contract belongs to COD-16 after the user's API-direction decision. Q1 sufficiency requires COD-25/F04 evidence after the resolution-scope decision. Numerical calibration, automatic ownership qualification, hosted trust/enforcement and exact implementation approval retain their existing downstream gates.

The reviewer did not rerun product suites, replay the supplied graph probe, audit all historical CI logs, verify hosted enforcement or independently repeat original-chat recovery. The review was read-only; no source, model, tracking or memory writes occurred. Reusable knowledge was assessed and no new memory record was warranted for unimplemented proposal contracts.

## Focused verification after the user's decisions

The same independent reviewer checked [cod17-decisions.md](cod17-decisions.md), SHA-256 `ff0f03f2e05de1a41872ea593e8e33cab4e02e2ab05ce170bbaf7147897cad70`, against the user's exact answers. The reviewed branch HEAD was `e84fdd91b3d9a0b50d2c4e081f0de4b0879e3925`; the decision record was then an untracked addition. All three original proposal input hashes remained unchanged.

Returned verdict: **Focused pass: no confirmed defects.** The record faithfully selects C1 and Q1, preserves compatibility and shared dispatch, retains `UNKNOWN` blocking and pilot sufficiency before calibration, and requires renewed user decisions for scope expansion. The reviewer also verified that this review record accurately described the earlier verdict and limits. Settings remained `default` / `gpt-6.1-sol` / `xhigh` / `fork_turns: none`.

This focused check covered decision/review records and hashes only. No source/probe review or product suites were repeated, and no writes occurred. The decisions resolve the earlier open alternatives; they do not authorise implementation or waive downstream gates.
