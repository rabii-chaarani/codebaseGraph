from __future__ import annotations

import json
import os
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path
from typing import Any

from codebase_graph import paths as graph_paths
from codebase_graph.ontology import ONTOLOGY_VERSION

CONFIG_NAME = graph_paths.CONFIG_NAME
DEFAULT_STATE_DIR = graph_paths.DEFAULT_STATE_DIR
MANIFEST_NAME = graph_paths.MANIFEST_NAME
MCP_SERVER_NAME = graph_paths.MCP_SERVER_NAME
GraphStatePaths = graph_paths.GraphStatePaths
derive_graph_state_paths = graph_paths.derive_graph_state_paths
SetupPaths = graph_paths.GraphStatePaths


def derive_setup_paths(repo_root: str | Path) -> SetupPaths:
    paths = derive_graph_state_paths(repo_root)
    if not paths.repo_root.exists():
        raise FileNotFoundError(f"Repository root does not exist: {paths.repo_root}")
    if not paths.repo_root.is_dir():
        raise NotADirectoryError(f"Repository root is not a directory: {paths.repo_root}")
    return paths


def build_setup_config(paths: SetupPaths, *, mcp_command: list[str]) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "repo_root": paths.repo_root.as_posix(),
        "repo_name": paths.repo_name,
        "state_dir": paths.state_dir.as_posix(),
        "database_path": paths.db_path.as_posix(),
        "manifest_path": paths.manifest_path.as_posix(),
        "ontology_version": ONTOLOGY_VERSION,
        "package_version": _package_version(),
        "mcp": {
            "server_name": MCP_SERVER_NAME,
            "command": list(mcp_command),
        },
    }


def load_setup_config(path: str | Path) -> dict[str, Any]:
    config_path = Path(path).expanduser().resolve()
    with config_path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    _validate_setup_config(payload, config_path)
    return payload


def write_setup_config(path: Path, payload: dict[str, Any]) -> str:
    previous = _read_json_if_exists(path)
    action = "created"
    if previous == payload:
        return "unchanged"
    if previous is not None:
        action = "updated"
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    with tmp_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")
    os.replace(tmp_path, path)
    return action


def _package_version() -> str:
    try:
        return version("codebase-graph")
    except PackageNotFoundError:
        return "0.1.0"


def _read_json_if_exists(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _validate_setup_config(payload: dict[str, Any], path: Path) -> None:
    required = ("repo_root", "repo_name", "database_path", "manifest_path")
    missing = [key for key in required if not payload.get(key)]
    if missing:
        joined = ", ".join(missing)
        raise ValueError(f"Invalid codebaseGraph setup config at {path}: missing {joined}")
