# Immutable release correction evidence

The independent verifier/reviewer reports, JUnit reports and supporting final-check files in this directory are unchanged copies of their captured originals. Their external paths are historical provenance. Product behavior and acceptance are recorded in the existing approval record and reports, not extended here.

## RED regression log packaging

`red-regression.normalized.log` is a **normalized copy, not the raw log**. Only trailing spaces/tabs/carriage returns on each line were stripped so the Git diff passes whitespace checks. The raw original remains unchanged outside the repository:

- Raw path: `/private/tmp/codebasegraph-immutable-release-production-20261029/red-regression.log`
- Raw SHA-256: `07f6efb9007c0a2f5edd6a8c498ddc59c81f17981cf45acf41d80ed2156e5719`
- Normalized copy SHA-256: `705a31b1aead94fd09127d65cdf93434e91f6cb5cd025927764f41b21810b91a`
- Actual command: `cargo test -p xtask --locked immutable_release_uploads_before_publication_and_retries_without_writes -- --nocapture`
- Environment: isolated task worktree with dedicated Production Cargo target; enhanced closed local mock-gh against the unchanged base workflow.
- Expected RED outcome: exit 101; `Immutable release rejects asset upload`, reproducing publication before upload. This is Production-provided evidence, not independent verifier execution.

No product source, independent report or JUnit bytes changed during this documentary packaging. The final reviewed product identities remain those in `final-source-inventory.json`, and all 52 independent tests passed. No live release action or v4.0.3 repair is claimed.
