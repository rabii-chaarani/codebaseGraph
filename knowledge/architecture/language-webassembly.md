---
description: Built-in WebAssembly Text discovery, vendored grammar, syntax-catalog, and semantic-normalization contract.
resource: repository-architecture
tags:
- architecture
- language-support
- parser
- tree-sitter
- webassembly
- wat
timestamp: 2026-08-25
title: WebAssembly Text Parsing Contract
type: architecture
---
# WebAssembly Text Parsing Contract

The Graph Runtime indexes WebAssembly Text files through a built-in profile backed by a pinned, vendored Tree-sitter WAT grammar.

## Public contract

- `webassembly` recognizes `.wat` files using `tree_sitter_wat@0.1.0+e3769473`.
- The language key is advertised by the CLI syntax catalog and MCP `graph_syntax` schema.
- Named modules, functions, and type definitions map to `Module`, `Function`, and `TypeAlias` ontology nodes.
- Imports normalize to `module.item`; exports use their declared string name; direct `call` and `return_call` instructions use the referenced index or identifier.
- Anonymous modules, functions, and type definitions remain available as ordered raw syntax without fabricated semantic identities.

Binary `.wasm` decoding, `.wast` scripts, validation, execution, and disassembly are outside this contract.

## Vendored grammar

The generated native parser, external scanner, headers, node catalog, license, and exact upstream provenance live under `vendor/tree-sitter-wat`. The grammar is pinned to upstream commit `e3769473b2d90643d8af500b5cfc2f25a674888a` and compiled by the package build script so crates.io source packages build offline.

The upstream payload-free scanner callbacks are locally corrected to return zero explicitly. Upstream and patched SHA-256 values are retained in the vendor provenance document. Grammar updates must replace the generated set atomically, preserve license and provenance, verify ABI compatibility, and pass parser, native, and crates.io package checks.

Related: [Graph Runtime](./graph-runtime.md), [Repository Ownership Map](./repository-map.md), and [Native Release Verification](./release-verification.md).
