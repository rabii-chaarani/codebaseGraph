# codebaseGraph

`codebase_graph` is a generic project/code knowledge graph engine for Python repositories. It scans a source root, builds a typed graph of files, modules, symbols, imports, calls, dependencies, entry points, and documentation sources, and exposes search, compact context, schema, and read-only query helpers.

## Install for local development

```bash
python -m pip install -e .[dev]
```

## Basic usage

```python
from codebase_graph import CodebaseGraph

graph = CodebaseGraph(source_root=".", state_dir=".codebase_graph/graph")
graph.materialize()
graph.search("FastAPI routes")
graph.context("SomeClass")
graph.cypher("MATCH (n:PythonClass) RETURN n.label LIMIT 5")
```

## CLI

```bash
codebase-graph status --source-root .
codebase-graph materialize --source-root .
codebase-graph schema
codebase-graph search "query"
codebase-graph context "SomeClass"
codebase-graph cypher "MATCH (n:PythonClass) RETURN n.label LIMIT 5"
```

The base package is intentionally small and importable without optional graph database or parquet bindings. Optional storage backends can be installed through extras as they mature.
