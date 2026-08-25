---
description: Built-in HTML discovery, grammar, syntax-catalog, and conservative graph-normalization contract.
resource: repository-architecture
tags:
- architecture
- html
- language-support
- parser
- tree-sitter
timestamp: 2026-08-25
title: HTML Parsing Contract
type: architecture
---
# HTML Parsing Contract

The Graph Runtime indexes HTML with a built-in Tree-sitter profile while preserving the distinction between raw markup syntax and higher-level application components.

## Public contract

- `html` recognizes `.html` and `.htm` using `tree_sitter_html@0.23.2`.
- The key is advertised by the CLI syntax catalog and MCP `graph_syntax` schema.
- Ordered raw syntax includes documents, doctypes, elements, tags, scripts, styles, attributes, and text.
- HTML elements are not promoted to ontology `Component` nodes because markup alone does not establish framework or component identity.

Embedded JavaScript and CSS injection parsing is outside this contract; standalone files are handled by their own language profiles.

Related: [Graph Runtime](./graph-runtime.md) and [Repository Ownership Map](./repository-map.md).