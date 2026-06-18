from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path


def resolve_native_product_binary(*, repo_root: Path | None = None, skip_current_script: bool = True) -> str | None:
    for candidate in _native_product_binary_candidates(repo_root=repo_root):
        resolved = candidate.expanduser().resolve()
        if skip_current_script and _is_current_script(resolved):
            continue
        if resolved.is_file() and os.access(resolved, os.X_OK):
            return resolved.as_posix()
    path_binary = shutil.which("codebase-graph")
    if path_binary is None:
        return None
    resolved_path_binary = Path(path_binary).expanduser().resolve()
    if skip_current_script and _is_current_script(resolved_path_binary):
        return None
    return resolved_path_binary.as_posix()


def default_native_product_command(*, repo_root: Path | None = None, skip_current_script: bool = True) -> str:
    return resolve_native_product_binary(repo_root=repo_root, skip_current_script=skip_current_script) or "codebase-graph"


def _native_product_binary_candidates(*, repo_root: Path | None) -> tuple[Path, ...]:
    candidates: list[Path] = []
    explicit = os.environ.get("CODEBASE_GRAPH_NATIVE_CLI")
    if explicit:
        candidates.append(Path(explicit))
    if repo_root is not None:
        candidates.extend(_repo_target_candidates(repo_root))
    package_repo_root = Path(__file__).resolve().parents[2]
    if repo_root is None or package_repo_root != repo_root.expanduser().resolve():
        candidates.extend(_repo_target_candidates(package_repo_root))
    return tuple(candidates)


def _repo_target_candidates(repo_root: Path) -> tuple[Path, Path]:
    root = repo_root.expanduser().resolve()
    return (
        root / "rust" / "target" / "release" / "codebase-graph",
        root / "rust" / "target" / "debug" / "codebase-graph",
    )


def _is_current_script(candidate: Path) -> bool:
    if not sys.argv or not sys.argv[0]:
        return False
    return candidate == Path(sys.argv[0]).expanduser().resolve()
