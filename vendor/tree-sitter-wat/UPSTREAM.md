# Vendored tree-sitter-wat grammar

- Upstream: https://github.com/g-plane/tree-sitter-wat
- Commit: `e3769473b2d90643d8af500b5cfc2f25a674888a`
- Upstream version metadata: `0.1.0`
- License: MIT (see `LICENSE`)
- Generated grammar ABI: 15

The repository vendors only the generated native parser inputs and node catalog required to build and describe the grammar offline. Generated files other than `src/scanner.c` were copied byte-for-byte from the pinned upstream commit. The scanner has a local safety correction: its payload-free `create` and `serialize` callbacks explicitly return zero instead of falling off a non-void function or claiming to have written an uninitialized byte. The upstream scanner SHA-256 before that correction is `669f14d7ab816551bce4511d4b39779713bfa5d51962ac2cc2a9c55e44280e69`. The MIT license text is unchanged apart from removing one redundant final blank line; its upstream SHA-256 is `0e373b1e533e7fdf342380a986890d494fcbb25cbc99beca2b46e6d3c726c4c9`.

| File | SHA-256 |
| --- | --- |
| `LICENSE` | `28a1c2b92a241a705632869e5c3a5e83a53bb182eb273c851565ae381f12982c` |
| `src/parser.c` | `ba3659413ddfa7acd66d1739ca96ace762e6d08f2c60a43d22670503fac87ec6` |
| `src/scanner.c` | `e6197b66843756e44e67ff6e9f61790b7df44c930ed1ba5b5fc1169874fc6d60` |
| `src/node-types.json` | `b8ee14131666f1553d8948fa661526b2aa5d4bc5f64aa5f28f662fb33ebf453a` |
| `src/tree_sitter/alloc.h` | `b29c1c9fb7cc82f58c84b376df1297d6e2737a1d655fd356db0859e3c29c2fea` |
| `src/tree_sitter/array.h` | `5bdf6ed1a78e3409fd443e085ca967a64c188a5d082aaf7f819bccd53a471c94` |
| `src/tree_sitter/parser.h` | `180b893c8734778fd32f372dfbc27bd6ad1cd2221f26150b31256ff6716320d2` |

When updating the grammar, pin an immutable upstream commit, replace all files as one set, refresh these hashes, verify the Tree-sitter ABI against the runtime, and rerun the parser, cross-platform, and crates.io package checks.
