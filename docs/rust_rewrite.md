# Rust Rewrite Architecture Baseline

## Objective

The Rust rewrite makes the native Rust binary the production runtime for shipped CLI and MCP entrypoints. Python remains only as the repository development and test harness; it must not own production CLI/MCP routing, release artifacts, package distribution, or fallback behavior.

## Native Extension Boundary

Use the Rust binary for production CLI and MCP execution. PyO3 bindings are retained only as optional compatibility
imports for local tests that explicitly load Python modules; the repository no longer exposes Python build metadata as a
package or distribution layer.

Initial native candidates are component internals only:

- Graph Builder: deterministic conversion from parser captures or normalized parse bundles into graph node and edge rows.
- Bulk Loader staging: deterministic staging table generation for LadyBugDB bulk inserts.
- Universal Tree-sitter Adapter: normalized capture extraction and parse bundle construction.
- Semantic enrichment batch engine: deterministic local-only symbol table, reference, call/type, and enrichment row construction.
- Scan/hash/diff helper: deterministic file snapshot hashing and manifest diff input preparation.

The Scryer model remains authoritative at the component level. Rust replaces selected operations inside the Materializer Orchestrator, Graph Builder, LadyBug Store Adapter, Universal Tree-sitter Adapter, and Semantic Enrichment components, and owns the production CLI/MCP surfaces.

## Python Development Harness Responsibilities

Python modules under `codebase_graph._native` retain development-only responsibilities:

- Import-compatible dataclass shapes and wrappers for retained tests and reviewed fixture updates.
- Historical benchmark tooling for migration evidence. Historical Python results may inform investigations, but they are not
  a release oracle for Rust-owned production behavior.
- Development-only support code needed by the Python test suite.
- Public dataclass shapes and compatibility wrappers for `ParseBundle`, `GraphBuildResult`, `CodeGraph`, `BulkLoadStats`, `MaterializationResult`, and manifest entries.
- Error translation from native failures into existing Python exceptions or diagnostics when a compatibility import calls into Rust.

## Rust-Owned Responsibilities

Rust may own deterministic batch kernels that operate on explicit inputs and return plain serializable outputs:

- Capture tables, normalized parser nodes, source text, paths, repository labels, and ontology names as inputs.
- Node rows, edge rows, diagnostics, unresolved symbols, staging rows, hash records, and diff records as outputs.
- Production CLI and MCP behavior, including argument parsing, output formatting, setup, graph retrieval, and stdio/HTTP MCP serving.
- No Notion behavior, Scryer behavior, or implicit network access.
- No direct change to LadyBugDB schema semantics unless a separate schema migration task approves it.

## Runtime and Fallback Policy

Native execution is the production default for shipped CLI and MCP entrypoints. Python fallback routing is not allowed for
Rust-owned commands.

Production entrypoints must:

1. Resolve the Rust product binary.
2. Fail explicitly when unsupported platforms or native runtime entrypoints are unavailable.
3. Preserve existing result shapes through the Rust implementation or documented compatibility shims.
4. Keep Python wrappers narrow, non-authoritative, and development-only.

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

`pyproject.toml` is limited to Python lint/test tool configuration. It must not define `[build-system]`, `[project]`,
`[project.scripts]`, `[tool.maturin]`, console scripts, or Python runtime dependencies. Production distributions are
built from `rust/crates/codebase_graph_native/Cargo.toml` as native binary archives.

## Build and Use

The native path is the default production surface for CLI and MCP entrypoints.

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

python scripts/benchmark_materialization.py \
  --repo-root . \
  --mode full \
  --iterations 3 \
  --warmups 1 \
  --output .codebaseGraph/benchmarks/materialization-native.json
```

Use `codebase-graph setup --repo-root .` for local validation. Production defaults are Rust-owned, and missing native entrypoints must fail explicitly instead of falling back to Python behavior.

Local-only semantic enrichment is part of the Rust native materialization batch. Provider-backed modes are not supported
by the Rust-only production materializer and must fail explicitly until ported.

## CI Expectations

Hosted CI should run:

- Python compatibility tests for retained wrappers.
- Rust-native integration tests for the production CLI, MCP, and materialization paths.
- `cargo test` for Rust unit tests.
- A native build smoke test on Linux, macOS, and Windows.
- Release-build benchmark evidence for the Rust production binary when performance claims or rollout decisions are made.

## Baseline Benchmark Command

Use the benchmark command when collecting migration or performance evidence:

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

- Last known Python baseline when comparing migration risk.
- Native release run from the production binary.
- Repository size, file counts, mode, iteration count, total elapsed time, per-iteration elapsed time, files/sec, nodes/sec, edges/sec, peak RSS when available, and graph summary counts.

## Benchmark Evidence - 2026-06-16

Local benchmark target: this repository at commit `c94fb44`, isolated state under `/private/tmp/codebasegraph-rust-bench`, one measured iteration, zero warmups, 2,380 scanned files, 121 rebuilt graph partitions for full mode. The Scryer architecture boundary is the Materializer Orchestrator: it scans files, computes manifest diffs, coordinates parser/graph builder work, writes graph store artifacts, and invokes semantic enrichment components. These results are therefore end-to-end materialization timings, not single-kernel microbenchmarks.

| Run | Semantic enrichment | Mean seconds | Files/sec | Nodes/sec | Edges/sec | Peak RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Python full | enabled | 37.709 | 63.11 | 4,896.89 | 8,680.60 | 4.10 GB |
| Python full | disabled | 19.918 | 119.49 | 9,271.06 | 12,372.02 | 2.72 GB |
| Historical native comparison full | disabled | 39.111 | 60.85 | 4,721.36 | 6,300.55 | 3.74 GB |
| Python no-change `changed` | disabled | 0.511 | 4,658.62 | 361,449.86 | 482,346.81 | 2.73 GB |
| Historical native comparison no-change `changed` | disabled | 0.608 | 3,911.93 | 303,516.32 | 405,035.79 | 3.73 GB |

The historical native comparison with semantic enrichment enabled did not complete within the practical comparison window and was interrupted after more than 180 seconds. The stack was waiting on the native semantic enrichment subprocess, so semantic enrichment was a blocker for native rollout on this repository at the time of that benchmark.

An attempted syntax-only Python full run against the local NumPy checkout was interrupted after more than 150 seconds while writing bulk JSON staging rows. That run did not produce comparable throughput metrics, but it shows that large mixed-language repositories need phase-level timing before assigning wins or losses to parser normalization, graph building, or bulk loader staging.

Observed cost drivers:

- Semantic enrichment adds about 17.8 seconds and 1.38 GB RSS to the Python full run on this repository. Native semantic enrichment is not ready for default use because the semantic-enabled native full benchmark failed to complete promptly.
- With semantic enrichment disabled, the historical native comparison full materialization was still about 1.96x slower than Python full materialization, so the remaining cost was in the combined parser normalization, graph builder, bulk staging, and Python/native serialization boundary.
- No-change `changed` mode is close in absolute terms, but native scan/hash/diff is still slower here: 0.608s versus 0.511s.

Historical rollout conclusion at the time: do not switch defaults based on the incomplete benchmark. That conclusion was superseded by the
2026-06-18 release-build benchmark below, where semantic-enabled native materialization completed with a measured speedup.

## Benchmark Evidence - 2026-06-18

Local benchmark target: this repository worktree, isolated state under `/private/tmp`, one measured iteration, zero
warmups. The native runs used a release Rust build; debug native builds are not representative and were slower than
Python on the same workload.

| Run | Semantic enrichment | Mean seconds | Files/sec | Nodes/sec | Edges/sec | Peak RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Python full | disabled | 55.172 | 936.11 | 6,169.47 | 8,549.84 | 3.62 GB |
| Native release full | disabled | 28.983 | 1,782.27 | 11,601.21 | 16,424.72 | 3.26 GB |
| Python full | enabled | 152.699 | 344.63 | 2,279.32 | 4,081.79 | 5.80 GB |
| Native release full | enabled | 45.314 | 1,161.54 | 7,590.19 | 16,460.88 | 5.23 GB |

Observed speedups on this checkout were about 1.90x for syntax-only full materialization and 3.37x for
semantic-enabled full materialization. The semantic-enabled native release run completed successfully, removing the
previous "did not complete" blocker for this repository. Continue to require release-build benchmark evidence before
making or validating rollout decisions.

## Golden Fixture Strategy

Golden fixtures are canonical graph-contract fixtures generated from reviewed Rust production output and explicit fixture
inputs. They must not depend on live filesystem state alone, and they must not use Python output as the pass/fail oracle.
Historical Python snapshots may be inspected during investigations, but release acceptance is based on the Rust production
path matching the reviewed fixtures and public graph contract.

Use three fixture levels:

- Unit fixtures: `ParseBundle` and capture-table fixtures for Graph Builder parity. Compare node IDs, edge IDs, labels, spans, qualified names, metadata, diagnostics, unresolved symbols, and summary counts.
- Integration fixtures: representative sample repositories materialized into isolated state directories. Compare `MaterializationManifest`, `MaterializationResult`, graph node counts, edge counts, typed relation counts, and selected query results.
- Regression fixtures: real bug-shape repositories or minimized reproductions for framework captures, semantic enrichment, manifest dependency evidence, unsupported files, and path normalization.

Canonicalization rules:

- Sort rows by stable ID before comparison.
- Normalize paths to repository-relative POSIX strings.
- Treat timestamps and elapsed durations as metadata outside parity unless a task explicitly covers them.
- Fail on count changes, ID changes, span changes, label changes, metadata loss, and diagnostic differences.

Production acceptance requires fixtures to pass for the Rust path and any retained development-only compatibility shims.

## Graph Compatibility Maintenance Rules

Future contributors must treat graph IDs, edge IDs, relation labels, source spans, manifest compatibility checks, and public dataclass shapes as stable compatibility surfaces. Changing any of them requires:

- Updating golden parity fixtures in the same change.
- Explaining whether existing `.codebaseGraph` state must be refreshed.
- Keeping production Rust output comparable with canonical golden fixtures for the same explicit input.
- Recording benchmark evidence before recommending a default change.

Do not change stable graph IDs in Rust to make implementation easier. The Rust path is an accelerator for the existing graph contract, not a new graph schema.

## Native Materializer Module Map

Current Rust module responsibilities after the native materializer cleanup:

- `lib.rs`: PyO3-callable syntax materialization orchestration and phase timing.
- `partition_builder.rs`: Per-file graph partition construction and manifest entry assembly.
- `graph_rows.rs`: Native typed node, edge, and built-row DTOs.
- `syntax_materializer.rs`: Row-first native syntax graph builder. It consumes `SyntaxNode` directly, uses hash-based internal dedup state, and emits sorted typed rows.
- `staging_writer.rs`: Native typed-row staging accumulator and LadyBug COPY staging file writer.
- `ladybug_writer.rs`: LadyBug schema and COPY execution.
- `hash.rs`: Native-owned stable partition IDs and file content hashes.
- `parser.rs`, `normalize.rs`, and `profiles.rs`: Tree-sitter parser integration and normalized syntax profiles.
- `scan.rs`: Native source snapshot and manifest diff helper.
- `semantic_enrichment.rs`: Row-first local semantic symbol resolution, call/type promotion, evidence edges, and
  semantic phase timings for the PyO3 materializer.
- `legacy_cli.rs`: Compatibility-only stdin protocol binary handlers.

Native materialization must not depend on `legacy_cli.rs`. Test-only imports from `legacy_cli.rs` are compatibility coverage
only; they are not production entrypoints or release oracles.

## Legacy Compatibility Surface

Caller audit on 2026-06-17:

| Protocol or helper | Current callers | Decision |
| --- | --- | --- |
| `BULK` | `src/codebase_graph/db/store.py` via `src/codebase_graph/_native/bulk_staging.py` | Compatibility wrapper only; not exposed by the production `codebase-graph` help surface. |
| `TSNORM` | `src/codebase_graph/ingest/tree_sitter_adapter.py` | Compatibility wrapper only; production setup/materialization uses Rust-owned parser normalization. |
| `SCAN` | `src/codebase_graph/ingest/materializer.py` | Compatibility wrapper only; production setup/materialization uses the Rust product CLI/PyO3 batch path. |
| `SEMANTIC` | `src/codebase_graph/semantic/enrichment_writer.py` | Compatibility wrapper only; production local-only semantic enrichment is Rust-owned in the native materializer. |
| `TREEGRAPH` | Rust compatibility tests only | Test support only for retained legacy protocol coverage. |
| `build_graph_output` | No callers | Deleted. |
| `write_bulk_staging_output` | No callers | Deleted. |

The compatibility protocol remains isolated behind `legacy_cli.rs`. It is not listed in production help and direct
`codebase-graph legacy-protocol` execution fails unless `CODEBASE_GRAPH_ENABLE_LEGACY_PROTOCOL=1` is set for
compatibility tests. Before deleting more protocols, replace or remove the Python shell-out callers above and rerun the
full Rust tests plus split Python materialization tests.
