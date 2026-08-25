---
description: Built-in TypeScript and TSX discovery, grammar, catalog, and semantic-normalization contract.
resource: repository-architecture
tags:
- architecture
- language-support
- parser
- tree-sitter
- typescript
- tsx
timestamp: 2026-08-25
title: TypeScript and TSX Parsing Contract
type: architecture
---
# TypeScript and TSX Parsing Contract

The Graph Runtime treats TypeScript and TSX as sibling built-in syntax languages so each file selects the correct Tree-sitter grammar and node catalog without changing the one-grammar-per-language profile contract.

## Public contract

- `typescript` recognizes `.ts`, `.mts`, and `.cts` using `tree_sitter_typescript@0.23.2`.
- `tsx` recognizes `.tsx` using the TSX grammar and node catalog from the same package version.
- Both keys are advertised by the CLI syntax catalog and MCP `graph_syntax` schema.
- TypeScript declarations, interfaces, enums, type aliases, functions, methods, imports, and calls map to established ontology nodes. JSX/TSX elements remain available through ordered raw syntax captures.

Framework-specific template semantics and embedded-language extraction are outside this contract.

Related: [Graph Runtime](./graph-runtime.md) and [Repository Ownership Map](./repository-map.md).