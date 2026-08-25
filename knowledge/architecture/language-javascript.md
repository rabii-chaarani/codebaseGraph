---
description: Built-in JavaScript and JSX discovery, grammar, catalog, and semantic-normalization contract.
resource: repository-architecture
tags:
- architecture
- javascript
- jsx
- language-support
- parser
- tree-sitter
timestamp: 2026-08-25
title: JavaScript and JSX Parsing Contract
type: architecture
---
# JavaScript and JSX Parsing Contract

The Graph Runtime exposes JavaScript as one built-in syntax language backed by the Tree-sitter JavaScript grammar, which includes JSX syntax.

## Public contract

- `javascript` recognizes `.js`, `.jsx`, `.mjs`, and `.cjs` using `tree_sitter_javascript@0.25.0`.
- The key is advertised by the CLI syntax catalog and MCP `graph_syntax` schema.
- Classes, functions, generator functions, methods, imports, and calls map to established ontology nodes.
- JSX elements remain available through ordered raw syntax captures; they are not promoted to framework-specific component nodes.

Framework inference and embedded-language extraction are outside this contract.

Related: [Graph Runtime](./graph-runtime.md) and [Repository Ownership Map](./repository-map.md).