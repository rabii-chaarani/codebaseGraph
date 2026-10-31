# Deterministic coordinator admission regression

Changed only repository source `src/coordinator/tests.rs:677`. Added `maintenance_busy_health_is_rejected_before_dispatch_and_recovers_after_release`.

- Uses existing `test_client` and its real loopback owner/dispatcher.
- Acquires the real owner's maintenance permit before the first health request.
- Sends `client.execute_invocation("health", ...)` over the existing client transport; requires exact `graph_busy`, `retryable=true`, and zero executor calls.
- A channel explicitly releases the maintenance permit. The test joins that release before issuing the second health request on the same client.
- Requires operation `health`, payload `ok=true`, and exactly one executor call after release.
- The scoped channel sender is RAII cleanup: assertion unwind drops it, allowing the holding thread to release its permit and the scope to join. No sleeps, polling, retries, or new timeout constants.
- This proves the admission contract and mechanism. It does not prove the original unlogged macOS response was `graph_busy`.

Patch: `coordinator-regression.patch` (external evidence directory).

SHA-256 of repository file bytes:

- Before: `aef769fe93e784aae97ac202e2d5b050791ac4c8d755b9ff15553d89ae31dc07`
- After: `a57902231b187e47b48188156eeb9133d7d431470bd4a9c6bc17633a40515d75`

Validation performed here: exact replacement matched once; resulting normalized text equals the previous file plus this single insertion; original CRLF retained. No build, test, formatting command, push, or other source edit was performed. Parent owns compilation/execution and `src/adapters/cli/tests/mcp.rs`.

Assumption: existing `test_client` is the intended real loopback admission harness; its executor is deliberately a counted successful fixture, not the real storage-backed health body. Constants changed: none.
