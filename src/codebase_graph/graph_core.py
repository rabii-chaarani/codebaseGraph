from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

from .code_map import CODE_EXTENSIONS, EXCLUDED_FILENAMES, MAX_INDEXED_FILE_BYTES, is_excluded_codebase_path_parts
from .graph_context import build_compact_graph_context
from .ladybug import HashingEmbeddingProvider, LadybugGraphExporter, LadybugGraphStore
from .ontology import schema_payload

DEFAULT_STATE_DIR = Path(".codebase_graph/graph")
DEFAULT_DB_FILENAME = "knowledge_graph.json"
DEFAULT_STAGING_DIRNAME = "staging"

@dataclass(slots=True)
class GraphCoreStatus:
    source_root: Path
    state_dir: Path
    database_path: Path
    staging_dir: Path
    database_exists: bool
    stale: bool
    source_file_count: int
    latest_source_mtime: float | None
    database_mtime: float | None

    def as_dict(self) -> dict[str, Any]:
        return {
            "source_root": str(self.source_root),
            "state_dir": str(self.state_dir),
            "database_path": str(self.database_path),
            "staging_dir": str(self.staging_dir),
            "database_exists": self.database_exists,
            "stale": self.stale,
            "source_file_count": self.source_file_count,
            "latest_source_mtime": self.latest_source_mtime,
            "database_mtime": self.database_mtime,
            "recommended_search_command": "codebase-graph search '<query>' --source-root .",
            "recommended_cypher_command": 'codebase-graph cypher "MATCH (n:PythonClass) RETURN n.id, n.label LIMIT 5" --source-root .',
            "recommended_schema_command": "codebase-graph schema",
            "recommended_context_command": "codebase-graph context '<anchor or task query>' --source-root .",
        }

class CodebaseGraph:
    def __init__(
        self,
        source_root: str | Path = ".",
        state_dir: str | Path | None = None,
        database_path: str | Path | None = None,
        staging_dir: str | Path | None = None,
        embedding_provider: HashingEmbeddingProvider | None = None,
    ) -> None:
        self.source_root = Path(source_root)
        self.state_dir = Path(state_dir) if state_dir is not None else self.source_root / DEFAULT_STATE_DIR
        self.database_path = Path(database_path) if database_path is not None else self.state_dir / DEFAULT_DB_FILENAME
        self.staging_dir = Path(staging_dir) if staging_dir is not None else self.state_dir / DEFAULT_STAGING_DIRNAME
        self.embedding_provider = embedding_provider or HashingEmbeddingProvider()

    def status(self) -> GraphCoreStatus:
        mtimes = _source_mtimes(self.source_root)
        database_exists = self.database_path.exists()
        database_mtime = self.database_path.stat().st_mtime if database_exists else None
        latest_source_mtime = max(mtimes) if mtimes else None
        stale = not database_exists or (
            latest_source_mtime is not None and database_mtime is not None and latest_source_mtime > database_mtime
        )
        return GraphCoreStatus(
            source_root=self.source_root,
            state_dir=self.state_dir,
            database_path=self.database_path,
            staging_dir=self.staging_dir,
            database_exists=database_exists,
            stale=stale,
            source_file_count=len(mtimes),
            latest_source_mtime=latest_source_mtime,
            database_mtime=database_mtime,
        )

    def materialize(self, overwrite: bool = True) -> dict[str, Any]:
        if self.database_path.exists() and not overwrite:
            raise ValueError(f"Graph database already exists: {self.database_path}")
        export = LadybugGraphExporter(self.source_root, embedding_provider=self.embedding_provider).build_export()
        self.staging_dir.mkdir(parents=True, exist_ok=True)
        store = LadybugGraphStore(self.database_path)
        store.write_export(export)
        return {"database_path": str(self.database_path), "summary": export.summary()}

    def ensure_current(self) -> dict[str, Any]:
        status = self.status()
        if status.stale:
            return self.materialize(overwrite=True)
        return {"database_path": str(self.database_path), "summary": {"status": "current"}}

    def schema(self) -> dict[str, Any]:
        return schema_payload()

    def search(self, query: str, limit: int = 10, refresh: bool = True, reinforce: bool = True) -> dict[str, Any]:
        if refresh:
            self.ensure_current()
        graph = self._read_graph()
        terms = _terms(query)
        scored: list[tuple[float, dict[str, Any]]] = []
        for node in graph.get("nodes", []):
            haystack = _node_text(node)
            score = sum(2.0 if term in str(node.get("label", "")).lower() else 1.0 for term in terms if term in haystack)
            if score > 0 or not terms:
                item = _compact_node(node)
                item["score"] = score
                scored.append((score, item))
        items = [item for _, item in sorted(scored, key=lambda pair: (-pair[0], pair[1].get("id", "")))[:limit]]
        return {
            "query": query,
            "items": items,
            "count": len(items),
            "database_path": str(self.database_path),
            "retrieval": "lexical_graph",
        }

    def context(
        self,
        query: str,
        *,
        kind: str | None = None,
        profile: str = "dependencies",
        limit: int = 3,
        max_depth: int = 1,
        budget: int = 600,
        include_raw: bool = False,
        refresh: bool = True,
    ) -> dict[str, Any]:
        if refresh:
            self.ensure_current()
        return build_compact_graph_context(
            self._read_graph(),
            query,
            kind=kind,
            profile=profile,
            limit=limit,
            max_depth=max_depth,
            budget=budget,
            include_raw=include_raw,
        )

    def cypher(self, query: str, parameters: dict[str, Any] | None = None, refresh: bool = True) -> dict[str, Any]:
        if refresh:
            self.ensure_current()
        if not _is_read_only_query(query):
            raise ValueError("Only read-only MATCH queries are supported")
        graph = self._read_graph()
        return _run_simple_match_query(graph, query, parameters or {})

    def _read_graph(self) -> dict[str, Any]:
        return LadybugGraphStore(self.database_path).read_export()

def main(argv: Sequence[str] | None = None) -> int:
    argv = _normalize_global_args(list(sys.argv[1:] if argv is None else argv))
    parser = argparse.ArgumentParser(description="Query or rebuild a generic codebase graph.")
    parser.add_argument("--source-root", default=".")
    parser.add_argument("--state-dir", default=None)
    parser.add_argument("--db-path", default=None)
    parser.add_argument("--staging-dir", default=None)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("status")
    materialize_parser = subparsers.add_parser("materialize")
    materialize_parser.add_argument("--no-overwrite", action="store_true")
    subparsers.add_parser("schema")
    search_parser = subparsers.add_parser("search")
    search_parser.add_argument("query")
    search_parser.add_argument("--limit", type=int, default=10)
    search_parser.add_argument("--no-refresh", action="store_true")
    context_parser = subparsers.add_parser("context")
    context_parser.add_argument("query")
    context_parser.add_argument("--kind")
    context_parser.add_argument("--profile", default="dependencies")
    context_parser.add_argument("--limit", type=int, default=3)
    context_parser.add_argument("--max-depth", type=int, default=1)
    context_parser.add_argument("--budget", type=int, default=600)
    context_parser.add_argument("--include-raw", action="store_true")
    context_parser.add_argument("--no-refresh", action="store_true")
    cypher_parser = subparsers.add_parser("cypher")
    cypher_parser.add_argument("query")
    cypher_parser.add_argument("--params-json", default="{}")
    cypher_parser.add_argument("--no-refresh", action="store_true")
    args = parser.parse_args(argv)
    core = CodebaseGraph(
        source_root=args.source_root,
        state_dir=args.state_dir,
        database_path=args.db_path,
        staging_dir=args.staging_dir,
    )
    if args.command == "status":
        payload = core.status().as_dict()
    elif args.command == "schema":
        payload = core.schema()
    elif args.command == "materialize":
        payload = core.materialize(overwrite=not args.no_overwrite)
    elif args.command == "search":
        payload = core.search(args.query, limit=args.limit, refresh=not args.no_refresh)
    elif args.command == "context":
        payload = core.context(
            args.query,
            kind=args.kind,
            profile=args.profile,
            limit=args.limit,
            max_depth=args.max_depth,
            budget=args.budget,
            include_raw=args.include_raw,
            refresh=not args.no_refresh,
        )
    elif args.command == "cypher":
        params = json.loads(args.params_json)
        if not isinstance(params, dict):
            raise ValueError("--params-json must decode to an object")
        payload = core.cypher(args.query, parameters=params, refresh=not args.no_refresh)
    else:
        parser.error(f"unsupported command: {args.command}")
    print(json.dumps(payload, indent=2, sort_keys=True))
    return 0


def _normalize_global_args(argv: list[str]) -> list[str]:
    value_flags = {"--source-root", "--state-dir", "--db-path", "--staging-dir"}
    extracted: list[str] = []
    remaining: list[str] = []
    index = 0
    while index < len(argv):
        item = argv[index]
        if item in value_flags and index + 1 < len(argv):
            extracted.extend([item, argv[index + 1]])
            index += 2
            continue
        remaining.append(item)
        index += 1
    return extracted + remaining

def _source_mtimes(source_root: Path) -> list[float]:
    if not source_root.exists():
        return []
    mtimes: list[float] = []
    for path in source_root.rglob("*"):
        if not path.is_file() or path.name in EXCLUDED_FILENAMES:
            continue
        try:
            rel_parts = path.relative_to(source_root).parts
        except ValueError:
            continue
        if is_excluded_codebase_path_parts(rel_parts):
            continue
        if path.suffix not in CODE_EXTENSIONS and path.suffix.lower() not in {".md", ".txt", ".rst", ".toml"}:
            continue
        try:
            if path.stat().st_size > MAX_INDEXED_FILE_BYTES:
                continue
            mtimes.append(path.stat().st_mtime)
        except OSError:
            continue
    return mtimes

def _terms(query: str) -> list[str]:
    return [term for term in re.split(r"[^a-zA-Z0-9_]+", query.lower()) if term]

def _node_text(node: dict[str, Any]) -> str:
    return " ".join(
        str(node.get(field, "")) for field in ("id", "table", "label", "kind", "path", "qualified_name", "summary")
    ).lower()

def _compact_node(node: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": node.get("id"),
        "table": node.get("table"),
        "label": node.get("label"),
        "kind": node.get("kind"),
        "path": node.get("path"),
        "qualified_name": node.get("qualified_name"),
        "line_start": node.get("line_start"),
        "summary": node.get("summary"),
    }

def _is_read_only_query(query: str) -> bool:
    lowered = query.strip().lower()
    return lowered.startswith("match ") and not any(
        token in lowered for token in (" create ", " merge ", " delete ", " set ", " drop ", " copy ", " load ")
    )

def _run_simple_match_query(graph: dict[str, Any], query: str, parameters: dict[str, Any]) -> dict[str, Any]:
    match = re.search(r"MATCH\s*\(\s*(\w+)\s*:\s*(\w+)\s*\)", query, flags=re.IGNORECASE)
    if not match:
        raise ValueError("Only simple MATCH (n:Label) queries are supported")
    variable, table = match.group(1), match.group(2)
    where = re.search(r"WHERE\s+(.+?)\s+RETURN", query, flags=re.IGNORECASE | re.DOTALL)
    return_match = re.search(r"RETURN\s+(.+?)(?:\s+LIMIT\s+(\d+))?\s*$", query, flags=re.IGNORECASE | re.DOTALL)
    if not return_match:
        raise ValueError("Query must include RETURN")
    columns = [column.strip() for column in return_match.group(1).split(",")]
    limit = int(return_match.group(2) or 100)
    rows: list[dict[str, Any]] = []
    for node in graph.get("nodes", []):
        if node.get("table") != table:
            continue
        if where and not _where_matches(node, variable, where.group(1), parameters):
            continue
        row: dict[str, Any] = {}
        for column in columns:
            if column == variable:
                row[column] = node
            elif column.startswith(f"{variable}."):
                field = column.split(".", 1)[1]
                row[column] = node.get(field)
            else:
                row[column] = node.get(column)
        rows.append(row)
        if len(rows) >= limit:
            break
    return {"query": query, "columns": columns, "rows": rows, "count": len(rows), "database_path": graph.get("database_path")}

def _where_matches(node: dict[str, Any], variable: str, expression: str, parameters: dict[str, Any]) -> bool:
    equals = re.match(rf"{re.escape(variable)}\.(\w+)\s*=\s*(.+)$", expression.strip())
    if not equals:
        return True
    field, raw_value = equals.group(1), equals.group(2).strip()
    if raw_value.startswith("$"):
        expected = parameters.get(raw_value[1:])
    else:
        expected = raw_value.strip("\'\"")
    return node.get(field) == expected

def _is_ladybug_lock_error(exc: BaseException) -> bool:
    return "lock" in str(exc).lower()

def _json_safe_value(value: Any) -> Any:
    try:
        json.dumps(value)
        return value
    except TypeError:
        return str(value)

if __name__ == "__main__":
    raise SystemExit(main())
