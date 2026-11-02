# Focused WI-0.1 correction report

- **Date:** 2026-11-02.
- **Scope:** Coordinator-requested wording corrections only; no new scope.
- **Original attempt preserved:** `/tmp/codebasegraph-workflow-implementation-f3002cbf84/attempt-report.md` (SHA256 `7dc4de0a30b7980ff5c425449c2d0afe3aad7d29bb91437c744e2aa66df7c2c6`) and `/tmp/codebasegraph-workflow-implementation-f3002cbf84/candidate.diff` (SHA256 `a1d8ebac602ec56a49f9f7fab4d578df408417849e2e53099b5f2056f9951717`). Neither was overwritten.
- **This correction:** `/tmp/codebasegraph-workflow-implementation-1e065617da`; `correction.diff` SHA256 `0bb0b24da218c1d2fb8a3eb13cd652279f3987df5df93452a0075c5246323a2c`.

## Focused corrections

1. Dispatch now names and freezes the complete affected input closure; the check command reads that stable closure and writers do not change it during the check. Unrelated work and safe reads remain allowed; no blanket reader ban.
2. Every ownership transfer requests the original writer to stop and requires confirmed writer/process/job/command and external-action quiescence, including a responsive writer. An authorized decisive host interrupt is required when steering cannot be consumed. If required host interrupt controls are unavailable, report the blocker and keep ownership pending. The five-minute observation is not a timeout or automatic termination.
3. The evidence manifest is written after payloads and lists their hashes, but does not hash itself.

## Affected-document checks

Markdown structure/link checks were run only for the changed dispatch and handoff documents: both fences balance, all local links resolve, and all handoff table rows retain three separators. No helper/product/test/discovery commands were rerun, per coordinator instruction.

## Final frozen candidate bytes

| Candidate after path | Bytes | SHA256 |
|---|---:|---|
| `sources/after/codebasegraph-engineer/SKILL.md.txt` | 14954 | `0491089d68a2dd791b2661e66b7250fd9ec045f2dd3bce75f1ccb0ddc912d9b7` |
| `sources/after/codebasegraph-engineer/references/dispatch-and-recovery.md.txt` | 15843 | `7d423e284798dfe52f1bb0b947729eafabcf3424f1b725551172f06284085cab` |
| `sources/after/codebasegraph-engineer/references/handoff-contract.md.txt` | 7133 | `0227ef3b27534d282f1e3b72a18aa7c5e2458bf349f528d44e027da751b8ff67` |

No live shared skill bytes changed. No Git mutation, build, external action, or long-running process was started. The candidate writer is now quiescent; no more candidate edits are planned. Evidence Verifier and fresh integrated Final Reviewer remain separate.
