# Native Prime Agent skill loader qualification

Date: 28 October 2026. Runtime: Prime Agent 0.10.1. Target: `.prime/agent/skills/codebasegraph-engineer/SKILL.md` in the isolated delivery worktree.

## Method and observations

The worker read installed skill-creator and native `prime-agent --help`, `prime-agent help prompt`, and `prime-agent --version`. Native `prime-agent prompt --cwd /Users/rabii/Projects/Repositories/codebaseGraph-engineer --json` assembles the fresh-root effective prompt without a model call. Before creation, the skill was absent. After creation, the command exited 0 and discovered one Markdown inventory entry at the intended project path; [inventory.xml](inventory.xml) retains that entry. No warning was observed; this does not rule out every shadowed lower-precedence duplicate.

The native print command with `--skill` and `/skill:codebasegraph-engineer` expanded actual SKILL.md into the user message. The explicit final response identified the display heading and four references and preserved read-only permission. Session `01a18102-b097-75ff-91ab-78073646d57c`, PID 91095, exit 0.

A separate fresh native invocation, without `--skill` or `/skill:`, received a codebaseGraph architecture/delegation read-only fixture. It selected `codebasegraph-engineer`, read actual SKILL.md and described its first authority check without starting work. Session `01a18102-f7d5-745c-a2e6-1a50faaa6d07`, PID 91121, exit 0. This is one observed model-directed routing success, not deterministic activation or production reliability.

Each original model run made exactly one `python_repl` call: `pathlib.read_text()` of the exact SKILL.md and printing it. Both returned the skill body successfully; no reference read was needed. The coordinator and independent final reviewer inspected the original complete event streams: no shell, product-source, Git, repository-rule, issue-service, integration, hook, extension, worker or write tool call was present. This is an event-stream audit, not an OS-level write/network trace.

## Durable evidence versus temporary audit files

Git-tracked evidence beside this report is deliberately focused:

- [commands.txt](commands.txt): exact two original commands/fixture prompts.
- [inventory.xml](inventory.xml): actual native project-skill discovery excerpt.
- [observed-execution-summary.json](observed-execution-summary.json): exact actual tool calls, final responses, session IDs and original-stream SHA-256 identities.
- [explicit-completed-events.jsonl](explicit-completed-events.jsonl) and [routing-completed-events.jsonl](routing-completed-events.jsonl): extracted completed assistant call/final events and successful tool-result identity, not full stdout or full returned body.
- [coordinator-audit.json](coordinator-audit.json): raw-output hash identities and separately disclosed redundant coordinator rerun observations.
- [../manifest.json](../manifest.json): actual skill/reference byte identities and environment.

The full native assembly output, original streaming stdout, installed-help output and preliminary snapshots existed at audit time under `/var/folders/3y/n_wglyqn3bv8xy8s0b9p32d00000gn/T/cbg-native-loader-y86scoiy/`, including `explicit-full.jsonl` and `routing-full.jsonl`. They are **temporary, not Git-retained delivery artifacts**. Original stdout SHA-256 values identify what was inspected; a hash does not retain bytes or recreate a missing stream. The durable extracts support the observed calls/answers; the reviewers' audit records attest the complete-stream inspection. Future readers must not assume the temporary files still exist or that Git contains every original event.

## Safety, recovery and limits

The runs used `--no-session`, a temporary session directory, `--offline` and an appended instruction allowing only skill/reference reads. This is an **instruction boundary, not a hard sandbox or disabled tools**. Native help exposed no no-tools switch. `--offline` disables startup network operations, not model/API calls; model API access occurred. No real issue/coding action was authorized. Actual inspected tool calls were read-only.

The worker's kernel restart briefly lost handle variables. Completed output was then recovered without worker rerun; earlier pending-response/routing statements are superseded by the actual evidence above. The coordinator started two bounded redundant reruns before that recovery notice; both completed with only permitted read/no-tool actions. They are disclosed separately, not counted as required extra coverage.

Original loader model: `openai-codex/gpt-6.1-sol`, effort not explicitly set/verified. These are loader/read-only checks, not required adversarial reviews. No expected answer was included in fixture prompts. The evidence does not qualify live integrations, coding agents, issue delivery, supported platforms or universal non-mutation. No repository/settings/package/integration changes were made by the worker; only temporary outputs. No reusable-memory claim follows from these fixtures.

SKILL.md observed SHA-256: `caa4cc326ff7b83e7758a636e3be7509a78812b70bd035a2141e6ae287e1f235`. Native discovery alone can be repeated with the documented `prime-agent prompt --cwd <delivery-worktree> --json`; no further model run is required for this handoff.
