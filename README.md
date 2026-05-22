# codebaseGraph

`codebaseGraph` is a generic project/code knowledge graph engine for coding repositories. The current filesystem materializer scans Python `.py` files, builds a typed graph of files, modules, symbols, imports, calls, dependencies, and entry points, and exposes search, compact context, schema, and read-only query helpers. The ontology and graph builder can represent documentation chunks from normalized parser or capture input, but Markdown and other documentation files are not scanned by the materializer yet.

## Install for local development

```bash
python -m pip install -e .[dev]
```
