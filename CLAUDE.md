<!-- k-wiki:start -->
## k-wiki workflow
- Use the `k_wiki` MCP server for every wiki interaction; do not invoke the `k-wiki` CLI or edit generated state directly.
- Treat `knowledge/` as curated repository intent, not a substitute for current code. Start with `wiki_list_bundles`, then `wiki_search_concepts`; use `wiki_get_concept`, `wiki_list_directory`, `wiki_get_backlinks`, and `wiki_get_neighborhood` to understand related decisions.
- Use the wiki for architecture, terminology, invariants, ownership, and prior decisions. Verify changeable details with codebase-graph MCP tools. If code and wiki conflict, identify the conflict and use `wiki_populate_page` to record clarified intent.
- Create missing pages with `wiki_create_page`; update existing pages with `wiki_populate_page`, supplying title, type, tags, useful Markdown, and `expected_content_hash`. Record durable decisions, public contracts, runbooks, invariants, and non-obvious trade-offs—not transient implementation noise or copied source.
- After meaningful wiki edits, call `wiki_validate` with `profile: recommended` and `include_structured_content: true`, then `wiki_check_links`. Call `wiki_build` with the configured `bundle_root` and `.kwiki/site` output root; it is a write operation.
- `knowledge/` is source and `.kwiki/` is generated state. Never manually edit generated projections.
- Use `wiki_get_diagnostics` to inspect remaining issues and `wiki_get_recent_changes` to understand recent work. In handoffs, cite updated concept paths and summarize decisions, uncertainties, and validation results.
<!-- k-wiki:end -->
