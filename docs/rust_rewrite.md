# Rust Rewrite Architecture Baseline

## Objective

The Rust rewrite accelerates hot, deterministic graph materialization work without changing the Python CLI, MCP server, graph query behavior, Notion workflow, or Scryer architecture boundaries. Python continues to own orchestration, state paths, setup, CLI/MCP routing, fallback policy, and user-facing result shapes. Rust modules are internal accelerators behind Python-callable wrappers.

## Native Extension Boundary

Use `pyo3` with `maturin` for the native extension once Rust code is introduced. The Python import surface should be an internal package such as `codebase_graph._native`; public callers continue to use the current Python modules.

Initial native candidates are component internals only:

- Graph Builder: deterministic conversion from parser captures or normalized parse bundles into graph node and edge rows.
- Bulk Loader staging: deterministic staging table generation for LadyBugDB bulk inserts.
- Universal Tree-sitter Adapter: normalized capture extraction and parse bundle construction.
- Semantic enrichment batch engine: deterministic local-only symbol table, reference, call/type, and enrichment row construction.
- Scan/hash/diff helper: deterministic file snapshot hashing and manifest diff input preparation.

The Scryer model remains authoritative at the component level. Rust replaces selected operations inside the Materializer Orchestrator, Graph Builder, LadyBug Store Adapter, Universal Tree-sitter Adapter, and Semantic Enrichment components; it does not create a new container or move ownership away from the Python CLI/MCP surfaces.

## Python-Owned Responsibilities

Python keeps ownership of:

- CLI and MCP argument parsing, output formatting, and JSON/block response contracts.
- `.codebaseGraph` path derivation, config loading, setup, and manifest write ordering.
- `GraphMaterializer` orchestration, atomic rebuild decisions, state cleanup, and fallback behavior.
- LadyBugDB connection lifecycle and schema setup.
- Public dataclass shapes and compatibility wrappers for `ParseBundle`, `GraphBuildResult`, `CodeGraph`, `BulkLoadStats`, `MaterializationResult`, and manifest entries.
- Error translation from native failures into existing Python exceptions or diagnostics.

## Rust-Owned Responsibilities

Rust may own deterministic batch kernels that operate on explicit inputs and return plain serializable outputs:

- Capture tables, normalized parser nodes, source text, paths, repository labels, and ontology names as inputs.
- Node rows, edge rows, diagnostics, unresolved symbols, staging rows, hash records, and diff records as outputs.
- No direct CLI behavior, MCP behavior, Notion behavior, Scryer behavior, persistent config mutation, or implicit network access.
- No direct change to LadyBugDB schema semantics unless a separate schema migration task approves it.

## Opt-In and Fallback Policy

Native execution is opt-in for v1 through `CODEBASE_GRAPH_NATIVE=1`. With the variable unset, Python implementations remain the default.

When `CODEBASE_GRAPH_NATIVE=1` is set:

1. Python imports `codebase_graph._native` lazily at the wrapper boundary.
2. Missing native modules, unsupported platforms, or native runtime failures fall back to the Python implementation unless a benchmark or test command explicitly asks for strict native execution.
3. Fallback paths must preserve the existing result shape and add diagnostics only where current callers already expose diagnostics.
4. Native outputs must be normalized by Python wrappers before they reach public APIs.

## Package Layout

Recommended first layout:

```text
src/codebase_graph/_native/
  __init__.py
  graph_builder.py
  bulk_loader.py
  tree_sitter_adapter.py
  semantic_enrichment.py
  scan_diff.py
rust/
  Cargo.toml
  crates/codebase_graph_native/
    Cargo.toml
    src/lib.rs
```

`pyproject.toml` should stay setuptools-based until the first native implementation task. Add `maturin` build metadata only when a compiled extension is introduced, and keep source distributions usable on platforms where a wheel is not available.

## Build and Use

The native path is a developer and benchmark surface until rollout evidence supports a default switch.

Build and validate the Rust helpers with:

```bash
cargo test --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml -- -D warnings
```

Run Python and native comparisons from the same checkout and isolated state directory:

```bash
python scripts/benchmark_materialization.py \
  --repo-root . \
  --mode full \
  --iterations 3 \
  --warmups 1 \
  --output .codebaseGraph/benchmarks/materialization-python.json

CODEBASE_GRAPH_NATIVE=1 python scripts/benchmark_materialization.py \
  --repo-root . \
  --mode full \
  --iterations 3 \
  --warmups 1 \
  --output .codebaseGraph/benchmarks/materialization-native.json
```

Use `CODEBASE_GRAPH_NATIVE=1 codebase-graph setup --repo-root .` only for local opt-in validation. Production defaults remain Python-owned, and native failures must either fall back to Python or surface as benchmark/test diagnostics without changing public result shapes.

## CI Expectations

Before enabling native defaults, hosted CI should run:

- Python test suite with `CODEBASE_GRAPH_NATIVE` unset.
- Python integration tests with `CODEBASE_GRAPH_NATIVE=1`.
- `cargo test` for Rust unit tests.
- A native build smoke test on Linux, macOS, and Windows.
- Benchmark comparison against the Python baseline produced by `scripts/benchmark_materialization.py`.

## Baseline Benchmark Command

Use the baseline command before replacing any Python default:

```bash
python scripts/benchmark_materialization.py \
  --repo-root . \
  --repo-root /path/to/large/repository \
  --mode full \
  --iterations 3 \
  --warmups 1 \
  --output .codebaseGraph/benchmarks/materialization-baseline.json
```

For no-change incremental timing, switch to `--mode changed`. The command uses isolated benchmark state so it does not overwrite the repository's normal `.codebaseGraph` database or manifest unless `--state-dir` points there intentionally.

Record at least:

- Current Python baseline with `CODEBASE_GRAPH_NATIVE` unset.
- Native opt-in run with `CODEBASE_GRAPH_NATIVE=1` once native code exists.
- Repository size, file counts, mode, iteration count, total elapsed time, per-iteration elapsed time, files/sec, nodes/sec, edges/sec, peak RSS when available, and graph summary counts.

## Benchmark Evidence - 2026-06-16

Local benchmark target: this repository at commit `c94fb44`, isolated state under `/private/tmp/codebasegraph-rust-bench`, one measured iteration, zero warmups, 2,380 scanned files, 121 rebuilt graph partitions for full mode. The Scryer architecture boundary is the Materializer Orchestrator: it scans files, computes manifest diffs, coordinates parser/graph builder work, writes graph store artifacts, and invokes semantic enrichment components. These results are therefore end-to-end materialization timings, not single-kernel microbenchmarks.

| Run | Semantic enrichment | Mean seconds | Files/sec | Nodes/sec | Edges/sec | Peak RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Python full | enabled | 37.709 | 63.11 | 4,896.89 | 8,680.60 | 4.10 GB |
| Python full | disabled | 19.918 | 119.49 | 9,271.06 | 12,372.02 | 2.72 GB |
| Native opt-in full | disabled | 39.111 | 60.85 | 4,721.36 | 6,300.55 | 3.74 GB |
| Python no-change `changed` | disabled | 0.511 | 4,658.62 | 361,449.86 | 482,346.81 | 2.73 GB |
| Native opt-in no-change `changed` | disabled | 0.608 | 3,911.93 | 303,516.32 | 405,035.79 | 3.73 GB |

The native opt-in full run with semantic enrichment enabled did not complete within the practical comparison window and was interrupted after more than 180 seconds. The stack was waiting on the native semantic enrichment subprocess, so semantic enrichment is currently a blocker for full native rollout on this repository.

An attempted syntax-only Python full run against the local NumPy checkout was interrupted after more than 150 seconds while writing bulk JSON staging rows. That run did not produce comparable throughput metrics, but it shows that large mixed-language repositories need phase-level timing before assigning wins or losses to parser normalization, graph building, or bulk loader staging.

Observed cost drivers:

- Semantic enrichment adds about 17.8 seconds and 1.38 GB RSS to the Python full run on this repository. Native semantic enrichment is not ready for default use because the semantic-enabled native full benchmark failed to complete promptly.
- With semantic enrichment disabled, native opt-in full materialization is still about 1.96x slower than Python full materialization, so the remaining cost is in the combined parser normalization, graph builder, bulk staging, and Python/native serialization boundary.
- No-change `changed` mode is close in absolute terms, but native scan/hash/diff is still slower here: 0.608s versus 0.511s.

Rollout conclusion: keep `CODEBASE_GRAPH_NATIVE=1` opt-in. Do not switch defaults from Python to Rust based on this evidence. Before revisiting defaults, rerun this benchmark on representative large repositories with multiple iterations, add phase-level attribution for parser normalization versus graph builder versus bulk loader, and require a semantic-enabled native full run to complete with parity and a measured speedup.

## Parity Fixture Strategy

Golden parity fixtures should compare Python and native output from the same explicit input, not from live filesystem state alone.

Use three fixture levels:

- Unit fixtures: `ParseBundle` and capture-table fixtures for Graph Builder parity. Compare node IDs, edge IDs, labels, spans, qualified names, metadata, diagnostics, unresolved symbols, and summary counts.
- Integration fixtures: representative sample repositories materialized into isolated state directories. Compare `MaterializationManifest`, `MaterializationResult`, graph node counts, edge counts, typed relation counts, and selected query results.
- Regression fixtures: real bug-shape repositories or minimized reproductions for framework captures, semantic enrichment, manifest dependency evidence, unsupported files, and path normalization.

Canonicalization rules:

- Sort rows by stable ID before comparison.
- Normalize paths to repository-relative POSIX strings.
- Treat timestamps and elapsed durations as metadata outside parity unless a task explicitly covers them.
- Fail on count changes, ID changes, span changes, label changes, metadata loss, and diagnostic differences.

Native replacement is allowed only after parity fixtures pass in both Python-default and native-opt-in modes.

## Graph Compatibility Maintenance Rules

Future contributors must treat graph IDs, edge IDs, relation labels, source spans, manifest compatibility checks, and public dataclass shapes as stable compatibility surfaces. Changing any of them requires:

- Updating golden parity fixtures in the same change.
- Explaining whether existing `.codebaseGraph` state must be refreshed.
- Keeping Python-default and `CODEBASE_GRAPH_NATIVE=1` outputs comparable for the same explicit input.
- Recording benchmark evidence before recommending a default change.

Do not change stable graph IDs in Rust to make implementation easier. The Rust path is an accelerator for the existing graph contract, not a new graph schema.
