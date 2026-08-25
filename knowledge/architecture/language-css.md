---
description: Built-in CSS discovery, grammar, syntax-catalog, and conservative graph-normalization contract.
resource: repository-architecture
tags:
- architecture
- css
- language-support
- parser
- tree-sitter
timestamp: 2026-08-25
title: CSS Parsing Contract
type: architecture
---
# CSS Parsing Contract

The Graph Runtime indexes CSS with a built-in Tree-sitter profile while keeping stylesheet syntax separate from application component identity.

## Public contract

- `css` recognizes `.css` using `tree_sitter_css@0.25.0`.
- The key is advertised by the CLI syntax catalog and MCP `graph_syntax` schema.
- Ordered raw syntax includes stylesheets, imports, rules, selectors, declarations, values, and function calls.
- CSS selectors are not promoted to ontology `Component` or definition nodes. Existing generic normalization may still represent unambiguous imports and calls.

SCSS, Sass, Less, CSS Modules inference, and embedded-style injection parsing are outside this contract.

Related: [Graph Runtime](./graph-runtime.md) and [Repository Ownership Map](./repository-map.md).