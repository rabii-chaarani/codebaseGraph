from __future__ import annotations

import argparse
import json
from collections.abc import Sequence
from pathlib import Path

from ingest import GraphMaterializer


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="codebase-graph")
    subparsers = parser.add_subparsers(dest="command", required=True)

    materialize_parser = subparsers.add_parser("materialize", help="Materialize the code graph")
    materialize_parser.add_argument("--source-root", default=".", help="Repository or source root to scan")
    materialize_parser.add_argument("--db", default=None, help="LadybugDB path; defaults under .codebase_graph")
    materialize_parser.add_argument("--manifest", default=None, help="Manifest path; defaults under .codebase_graph")
    materialize_parser.add_argument("--mode", choices=("full", "changed"), default="changed")
    materialize_parser.add_argument("--no-fts", action="store_true", help="Skip FTS index creation")

    args = parser.parse_args(argv)
    if args.command == "materialize":
        materializer = GraphMaterializer(
            Path(args.source_root),
            db_path=args.db,
            manifest_path=args.manifest,
            include_fts=not args.no_fts,
        )
        result = materializer.materialize(mode=args.mode)
        print(json.dumps(_result_payload(result), indent=2, sort_keys=True))
        return 0
    parser.error(f"Unknown command: {args.command}")
    return 2


def _result_payload(result: object) -> dict[str, object]:
    return {
        "mode": getattr(result, "mode"),
        "scanned": getattr(result, "scanned"),
        "rebuilt": getattr(result, "rebuilt"),
        "skipped": getattr(result, "skipped"),
        "deleted": getattr(result, "deleted"),
        "diagnostics": list(getattr(result, "diagnostics")),
        "manifest_path": getattr(result, "manifest_path"),
        "rebuilt_paths": list(getattr(result, "rebuilt_paths")),
        "skipped_paths": list(getattr(result, "skipped_paths")),
        "deleted_paths": list(getattr(result, "deleted_paths")),
        "graph_summary": dict(getattr(result, "graph_summary")),
    }


__all__ = ["main"]
