# CBG-COPY-01 — Prior investigation evidence

This is a compact provenance record for the proposed fix, not executed verification of an implementation. The planning task ran no builds, native fault experiments, production writes or tests. Scope and approval remain in [spec.md](spec.md).

## Decision-relevant observations already executed

- Inspected CodebaseGraph source: `6aa5ba8bd99c3071e106a8bfc7a408652a8e4bb5`. Its writer/staging/pool calculation also compared equal to local `origin/main` `ca115690b226384164cf364458a0e3255b8908d3`; this does not establish lifecycle equivalence or identify the original incident executable.
- Host: macOS arm64. Temporary databases and isolated project copy only; official released native archives were SHA-verified.
- With official Ladybug 0.22.0, a Rust project copy changed only to `lbug = "=0.22.0"` and the corresponding lockfile passed Cargo check including tests and two focused native writer tests. This is integration evidence on one host, not approval to upgrade or a full-platform qualification.
- 0.22 retained the exact second unindexed COPY guard. Separate sequential COPY files with an early primary-key hash index worked. A JSON file-list COPY silently loaded only the first file in the evaluated fixture; do not substitute list/glob syntax for per-file COPY.
- In 0.22, suppressing automatic checkpoints let COPY report success but lose copied rows after abrupt `_exit` before close. Keeping the default automatic checkpoint retained the fixture's rows. This is one tested boundary, not an exhaustive durability proof.
- Official 0.22.0 checkpoint integrity probe twice reproduced [upstream #1406](https://github.com/LadybugDB/ladybug/issues/1406): a fixture child used `RLIMIT_FSIZE` of current file size + 256 KiB to force a CHECKPOINT I/O failure, reset the limit, then CHECKPOINT returned success. A primary-key lookup found `id=7` before close. A new read-only process saw 20 rows and a forced scan found `id=7`, while primary-key lookup returned zero. This supports lookup-vs-scan regression oracles; it does not prove the 0.19 incident's first failure or require disk-full tests in routine CI.
- The same synthetic failure workload under 0.19 prevented read-only reopen with a checkpoint-in-progress diagnostic. This is not evidence that 0.19 recovery is safe or that 0.22's failure is a new regression. The proposed fix avoids treating either failure as permission to retry a mutating statement.
- A toy 0.19 database opened read-only in 0.22 unchanged; after a 0.22 write/checkpoint, 0.19 could not reopen the upgraded copy. A legacy non-ASCII STRING primary-key fixture required rebuilding stale hashes. Migration/rebuild/rollback and matching extensions remain separate upgrade decisions.

## Exact checkpoint observation excerpts

```text
CHECKPOINT -> STATE:0
IO exception: Cannot write to file ... Error: File too large
LIMIT_RESET
CHECKPOINT -> STATE:1
before-close MATCH (p:Person {id:7}) RETURN count(*) -> 1
```

```text
fresh-process readonly:
MATCH (p:Person) RETURN count(*) -> 20
MATCH (p:Person) WITH p.id AS id WHERE id=7 RETURN count(*) -> 1
MATCH (p:Person {id:7}) RETURN count(*) -> 0
```

The child-only fault used synthetic scratch data. No live graph, daemon, production resource limit, project dependency or repository code changed during that evaluation.

## Source and scope limits

- Bulk COPY/checkpoint distinction: [PR1204](https://github.com/LadybugDB/ladybug/pull/1204) and [PR1361](https://github.com/LadybugDB/ladybug/pull/1361). PR1361 was unmerged at evaluation; no fix from an open PR is claimed as released.
- Evaluated latest stable: [v0.22.0](https://github.com/LadybugDB/ladybug/releases/tag/v0.22.0), published 2026-10-25, commit `4ca22377b85a33a10247410fe3944a5224b1169d`.
- Original DocumentationSource incident's first-attempt logs, exact binary and COPY plan were unavailable. Same-candidate replay is an established exposure, not an established incident diagnosis.
- Cross-platform qualification, representative workload limits and the proposed fail-closed implementation have not been executed by this planning work.

## Local raw artifacts

The compact observations above are portable. Local paths below are diagnostic pointers and hashes, **not** a substitute for retained final implementation JUnit/native-platform evidence. They may disappear with temporary storage.

Original evaluation directory: `/private/tmp/ladybug-022-evaluation-dp_8zy5d`; final research assessment `evaluation.md`; native/crate inputs `provenance.json`; detailed engine/integration reports `engine-evaluation.md` and `integration-evaluation.md`.

| Existing raw file | SHA-256 at planning readback |
| --- | --- |
| `checkpoint-integrity-observations.json` | `3b9ef4730ca17b2501ff3995b7fb25f166b98abc279717dd67c7752d49cd9684` |
| `failed-checkpoint-confirm.log` | `1a4000b2f68bbf88dfa50de8666adfd4e556ef6efc821336c24b0042931fd1de` |
| `cargo-focused-tests.log` | `306fe8982b34bf22cc6c44416904908b89d9dc15c1140cc84eac77d46f1ad38f` |
| `cargo-check.log` | `54a4d41815e508e67691e7280f4bd47365c4be51864e6d33b3f873f5cf9e8a91` |
| `no-close-observations.json` | `bd49c9cedd051e41ea68cab99426dc61f6a8f40c9ef61e8d6e5a77d7f6286cdf` |
