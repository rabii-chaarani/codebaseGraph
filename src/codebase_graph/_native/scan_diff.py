from __future__ import annotations

import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


class NativeScanDiffUnavailable(RuntimeError):
    """Raised when strict native scan/diff is requested but unavailable."""


@dataclass(frozen=True)
class NativeSourceSnapshot:
    path: str
    absolute_path: str
    content_hash: str
    language: str | None


@dataclass(frozen=True)
class NativeManifestDiff:
    added: tuple[str, ...]
    modified: tuple[str, ...]
    unchanged: tuple[str, ...]
    deleted: tuple[str, ...]
    force_rebuild: bool


@dataclass(frozen=True)
class NativeScanDiffResult:
    snapshots: tuple[NativeSourceSnapshot, ...]
    diagnostics: tuple[str, ...]
    diff: NativeManifestDiff | None


def scan_repository(payload: str, *, strict: bool = False) -> NativeScanDiffResult | None:
    """Run the native repository scan/hash/manifest-diff helper for an encoded payload."""
    if not strict and os.environ.get("CODEBASE_GRAPH_NATIVE") != "1":
        return None

    command = _native_command(strict=strict)
    if command is None:
        if strict:
            raise NativeScanDiffUnavailable("native scan/diff command is unavailable")
        return None

    try:
        completed = subprocess.run(
            command,
            input=payload,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=True,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        if strict:
            stderr = getattr(exc, "stderr", "")
            raise NativeScanDiffUnavailable(f"native scan/diff failed: {stderr or exc}") from exc
        return None

    return _decode_result(completed.stdout)


def _native_command(*, strict: bool) -> list[str] | None:
    configured = os.environ.get("CODEBASE_GRAPH_NATIVE_SCAN_DIFF")
    if configured:
        return [configured]

    binary = _built_binary_path()
    if binary.exists():
        return [binary.as_posix()]

    if strict and RUST_MANIFEST.exists() and shutil.which("cargo"):
        return [
            "cargo",
            "run",
            "--quiet",
            "--manifest-path",
            RUST_MANIFEST.as_posix(),
            "--bin",
            NATIVE_BINARY_NAME,
        ]
    return None


def _built_binary_path() -> Path:
    suffix = ".exe" if sys.platform.startswith("win") else ""
    return REPO_ROOT / "rust" / "target" / "debug" / f"{NATIVE_BINARY_NAME}{suffix}"


def _decode_result(output: str) -> NativeScanDiffResult:
    snapshots: list[NativeSourceSnapshot] = []
    diagnostics: list[str] = []
    diff: NativeManifestDiff | None = None
    result_count: int | None = None

    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        match parts[0]:
            case "RESULT" if len(parts) == 2:
                result_count = int(parts[1])
            case "SNAP" if len(parts) == 5:
                language = _unhex(parts[4])
                snapshots.append(
                    NativeSourceSnapshot(
                        path=_unhex(parts[1]),
                        absolute_path=_unhex(parts[2]),
                        content_hash=_unhex(parts[3]),
                        language=language or None,
                    )
                )
            case "DIAG" if len(parts) == 2:
                diagnostics.append(_unhex(parts[1]))
            case "DIFF":
                diff = _decode_diff(parts)
            case other:
                raise ValueError(f"Unknown native scan/diff record: {other}")

    if result_count is None:
        raise ValueError("Native scan/diff did not report snapshot count")
    if result_count != len(snapshots):
        raise ValueError("Native scan/diff snapshot count mismatch")

    return NativeScanDiffResult(snapshots=tuple(snapshots), diagnostics=tuple(diagnostics), diff=diff)


def _decode_diff(parts: list[str]) -> NativeManifestDiff:
    cursor = 2
    if len(parts) < cursor:
        raise ValueError("Native scan/diff DIFF record is incomplete")
    force_rebuild = parts[1] == "1"
    added = _decode_hex_list(parts, cursor)
    cursor += len(added) + 1
    modified = _decode_hex_list(parts, cursor)
    cursor += len(modified) + 1
    unchanged = _decode_hex_list(parts, cursor)
    cursor += len(unchanged) + 1
    deleted = _decode_hex_list(parts, cursor)
    cursor += len(deleted) + 1
    if cursor != len(parts):
        raise ValueError("Native scan/diff DIFF record has trailing fields")
    return NativeManifestDiff(
        added=tuple(added),
        modified=tuple(modified),
        unchanged=tuple(unchanged),
        deleted=tuple(deleted),
        force_rebuild=force_rebuild,
    )


def _decode_hex_list(parts: list[str], cursor: int) -> list[str]:
    count = int(parts[cursor])
    values = parts[cursor + 1 : cursor + 1 + count]
    if len(values) != count:
        raise ValueError("Native scan/diff hex list count mismatch")
    return [_unhex(value) for value in values]


def _unhex(value: str) -> str:
    return bytes.fromhex(value).decode("utf-8")
