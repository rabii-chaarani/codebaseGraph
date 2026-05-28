from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

DEFAULT_STATE_DIR = ".codebaseGraph"
CONFIG_NAME = "config.json"
MANIFEST_NAME = "manifest.json"
MCP_SERVER_NAME = "codebase_graph"


@dataclass(frozen=True, slots=True)
class GraphStatePaths:
    repo_root: Path
    repo_name: str
    state_dir: Path
    db_path: Path
    manifest_path: Path
    config_path: Path

    def as_dict(self) -> dict[str, str]:
        return {
            "repo_root": self.repo_root.as_posix(),
            "repo_name": self.repo_name,
            "state_dir": self.state_dir.as_posix(),
            "db_path": self.db_path.as_posix(),
            "manifest_path": self.manifest_path.as_posix(),
            "config_path": self.config_path.as_posix(),
        }


def derive_graph_state_paths(repo_root: str | Path) -> GraphStatePaths:
    root = Path(repo_root).expanduser().resolve()
    repo_name = _repo_name(root)
    state_dir = root / DEFAULT_STATE_DIR
    return GraphStatePaths(
        repo_root=root,
        repo_name=repo_name,
        state_dir=state_dir,
        db_path=state_dir / f"{repo_name}_graph.ldb",
        manifest_path=state_dir / MANIFEST_NAME,
        config_path=state_dir / CONFIG_NAME,
    )


def _repo_name(root: Path) -> str:
    name = root.name.strip()
    if name:
        return _safe_name(name)
    return "repository"


def _safe_name(value: str) -> str:
    normalized = "".join(character if character.isalnum() or character in {"-", "_"} else "_" for character in value)
    return normalized.strip("._-") or "repository"
