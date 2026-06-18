from __future__ import annotations

from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover - Python 3.10 compatibility
    import tomli as tomllib


def rust_package_version() -> str:
    manifest_path = Path(__file__).resolve().parents[2] / "rust" / "crates" / "codebase_graph_native" / "Cargo.toml"
    payload = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    version = payload.get("package", {}).get("version")
    if not isinstance(version, str) or not version:
        raise RuntimeError(f"Rust package version is missing from {manifest_path}")
    return version
