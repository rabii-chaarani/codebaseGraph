# API and Adapter Boundary Centralization

## Scryer change

- Change: `chg-1`
- Intent: centralize request normalization, repository runtime resolution,
  materialization preparation, and refresh policy under the transport-neutral
  API; reduce CLI and MCP adapters to transport parsing and response framing.
- Model status: structurally valid; implementation remains pending.

## Scope

### In scope

- Add canonical request defaulting and semantic validation under `src/api/**`.
- Resolve one repository runtime context per public operation under
  `src/api/**`.
- Prepare materialization inputs, configuration, and manifest persistence under
  `src/api/**`.
- Share refresh batching, filtering, retry/backoff, watcher selection, and
  refresh state between CLI and MCP under `src/api/**`.
- Remove reverse dependencies from `src/api/**` into `src/cli/**`.
- Keep CLI and MCP modules responsible for transport syntax, invocation, and
  output/protocol framing.
- Preserve existing CLI flags, MCP tool names and schemas, result shapes, error
  codes, and observable refresh behavior.

### Out of scope

- Changing graph query or materialization engine semantics.
- Renaming CLI commands, flags, or MCP tools.
- Changing storage schemas or graph database formats.
- Introducing a new dependency.
- Redesigning user-facing output.

## Target ownership

| Concern | API owner | Adapter responsibility |
| --- | --- | --- |
| Request defaults and semantic validation | `Request Normalizer` in `src/api/normalization.rs` | Parse CLI arguments or MCP JSON into public request contracts |
| Repository paths and execution context | `Repository Runtime Resolver` in `src/api/context.rs` | Pass repository selection from the transport |
| Materialization input preparation and manifest persistence | `Materialization API` in `src/api/materialization.rs` | Parse build/plan inputs and frame results |
| Refresh filtering, batching, retries, watcher choice, and state | `Repository Refresh Service` in `src/api/refresh.rs` | Parse watch/refresh inputs and frame status/events |
| Public result formatting primitives | Existing API presenter/catalog components | Render terminal or MCP protocol envelopes only |

## Requirements

1. Every public operation enters the facade as a public request contract, then
   receives canonical defaults and semantic validation exactly once.
2. Runtime resolution produces one canonical source root, graph path,
   configuration, and manifest context for the selected repository.
3. Materialization preparation reads and converts request/configuration inputs
   once, and both build and dry-run plan execution consume that prepared input.
4. CLI watch and MCP auto-refresh use the same event filtering, batch bounds,
   retry classification, backoff, watcher selection, and refresh state model.
5. `src/api/**` has no dependency on `src/cli/**`; adapter modules do not call
   graph, materialization, or refresh implementation internals directly.
6. Transport adapters retain only syntax/protocol parsing, invocation of the
   public API, and response framing.

## Acceptance criteria

- `rg -n "crate::cli" src/api` returns no matches.
- Boundary tests fail if API modules import CLI modules or if CLI/MCP transport
  modules bypass the facade to invoke graph, materialization, or refresh
  internals.
- Equivalent CLI and MCP requests normalize to the same public request and
  produce the same `ApiError` code for invalid semantic input.
- MCP generated schemas and runtime validation agree on required fields and
  accepted defaults.
- Repository selection is resolved once per operation; graph reads,
  materialization, health, and refresh use the same resolved graph and manifest
  paths.
- Materialization preparation does not duplicate configuration reads, source
  scans, include/exclude resolution, graph builds, or manifest writes.
- CLI watch and MCP refresh pass the same tests for event filtering,
  coalescing, transient retry/backoff, permanent failures, and status
  transitions.
- Existing CLI output snapshots and MCP protocol/result tests remain unchanged
  unless an existing test encodes the architectural defect itself.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  and `cargo test --all-targets --all-features` pass.
- Scryer `chg-1` is folded only with source anchors and genuine tests attached
  to each conditional symbol claim.

## Implementation sequence

### 1. Lock the transport-neutral behavior

Add or strengthen regression coverage before moving ownership:

- Extend `src/api/boundary_tests.rs` with both dependency directions:
  API must not import CLI, and CLI/MCP must not bypass the public facade.
- Extend `src/cli/tests/graph.rs` and `src/cli/tests/mcp.rs` with equivalent
  request/error cases.
- Extend `src/cli/tests/dispatch_materialize.rs` with build-versus-plan
  preparation parity and single-preparation assertions.
- Extend `src/cli/tests/watch.rs` with shared policy cases for filtering,
  batching, retry, and state transitions.
- Preserve repository auto-detection coverage in `tests/auto_repo_root.rs`.

These tests establish the compatibility baseline for the ownership move.

### 2. Centralize request normalization

- Create `src/api/normalization.rs` implementing the planned
  `normalize_request` and `validate_request` symbols over public contracts from
  `src/api/contracts.rs`.
- Route all facade/core dispatch through normalization in `src/api/core.rs` and
  `src/api/facade.rs`.
- Keep only syntactic extraction in `src/cli/graph/options.rs`,
  `src/cli/watch/options.rs`, and `src/cli/mcp/tools.rs`.
- Generate or validate MCP schemas from the same public contract rules so
  required fields cannot diverge from runtime behavior.
- Remove adapter-local semantic defaults and validation after parity tests pass.

### 3. Make runtime resolution canonical

- Expand `src/api/context.rs` into the planned Repository Runtime Resolver,
  owning `RepoRuntime` and `resolve_runtime`.
- Move repository source, graph, configuration, and manifest path selection out
  of CLI helpers.
- Delete the separate
  `src/cli/graph/health.rs::resolve_health_runtime` path and route health through
  the same resolver.
- Update materialization and refresh entry points to accept the resolved API
  context rather than recomputing paths.
- Verify explicit repository selection and auto-detected repository selection
  resolve identically across CLI and MCP.

### 4. Move materialization preparation into the API

- Consolidate request conversion, configuration preparation, source metadata,
  and manifest persistence in `src/api/materialization.rs`.
- Move the responsibilities currently implemented by
  `src/cli/materialization/request.rs` and
  `src/cli/materialization/manifest.rs` behind public API inputs and outputs.
- Keep `src/cli/materialization/command.rs` limited to parsing build/plan
  transport input, invoking the facade, and selecting an output frame.
- Keep `src/cli/materialization/output.rs` limited to terminal presentation.
- Ensure build and dry-run plan share one preparation path and neither performs
  a hidden second graph build or configuration read.

### 5. Introduce the shared refresh service

- Create `src/api/refresh.rs` for the planned Repository Refresh Service.
- Move event filtering, batch collection, polling/native watcher coordination,
  retry classification/backoff, and refresh state from `src/cli/watch/**` and
  `src/cli/mcp/refresh.rs`.
- Expose transport-neutral one-shot and continuous refresh operations through
  public contracts and the facade.
- Reduce `src/cli/watch/command.rs` to watch-command parsing, API invocation, and
  event/status framing.
- Reduce `src/cli/mcp/refresh.rs` to MCP lifecycle wiring and protocol framing;
  it must not own a separate refresh algorithm or policy.
- Preserve cancellation, shutdown, and read/write coordination semantics with
  focused concurrency tests.

### 6. Remove the remaining reverse dependencies

- Move any reusable catalog or presentation implementation still imported from
  `src/cli/**` behind `src/api/catalog.rs` and `src/api/presenter.rs`.
- Re-export the new API services through `src/api/mod.rs`.
- Delete obsolete adapter wrappers and duplicate helpers only after callers and
  regression tests have migrated.
- Run the boundary checks before each deletion to avoid replacing direct
  duplication with a dependency cycle.

### 7. Verify and close the Scryer change

Run, in order:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
rg -n "crate::cli" src/api
```

Then:

- Re-run Scryer structural validation.
- Inspect drift for every moved symbol.
- Fold `chg-1` responsibility-by-responsibility with exact source anchors.
- Attach tests for request rejection, runtime resolution, refresh batch
  coalescing, and transient refresh retry to their conditional claims.
- Confirm the pending queue for `chg-1` is empty and the committed model is
  healthy.

## Risks and mitigations

- **Transport behavior drift:** lock CLI snapshots and MCP result/error tests
  before moving code.
- **Over-generalized normalization:** normalize public contracts by operation;
  do not build a stringly typed transport abstraction.
- **Runtime path divergence:** make resolved runtime context immutable for the
  duration of an operation and pass it downward.
- **Refresh lifecycle regressions:** preserve cancellation and guard ownership
  with deterministic state-transition and concurrency tests.
- **Materialization double work:** instrument preparation in tests and assert a
  single preparation/build path.
- **Dependency-cycle substitution:** enforce both API-to-adapter and
  adapter-to-internal boundary tests continuously.

## Scryer model delta

The plan adds three API components:

- `Request Normalizer` (`node-142`)
- `Repository Runtime Resolver` (`node-143`)
- `Repository Refresh Service` (`node-144`)

It moves runtime, materialization-preparation, and refresh symbols to those API
owners, rewords CLI/MCP adapter responsibilities around parsing and framing,
deletes the duplicate health runtime resolver, and links both adapters to the
shared refresh service. The pending implementation queue is intentionally kept
under `chg-1`.
