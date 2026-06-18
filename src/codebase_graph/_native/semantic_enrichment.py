from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"


class NativeSemanticBatchUnavailable(RuntimeError):
    """Raised when strict native semantic enrichment is requested but unavailable."""


@dataclass(frozen=True)
class NativeSemanticEdge:
    graph_index: int
    id: str
    type: str
    source_id: str
    target_id: str
    kind: str
    confidence: float
    metadata: dict[str, Any]


@dataclass(frozen=True)
class NativeSemanticEvidence:
    evidence_id: str
    source: str
    confidence: float
    provider: str
    diagnostics: tuple[str, ...]
    metadata: dict[str, Any]


@dataclass(frozen=True)
class NativeSemanticEvidenceLink:
    graph_index: int
    semantic_relation_id: str
    evidence_node_id: str
    evidence_kind: str
    confidence: float
    metadata_fallback: bool


@dataclass(frozen=True)
class NativeSemanticFallback:
    graph_index: int
    semantic_relation_id: str
    source_node_id: str
    evidence_id: str
    metadata: dict[str, Any]


@dataclass(frozen=True)
class NativeSemanticBatchResult:
    symbol_count: int
    call_type_relations: int
    edges: tuple[NativeSemanticEdge, ...]
    evidence: tuple[NativeSemanticEvidence, ...]
    evidence_links: tuple[NativeSemanticEvidenceLink, ...]
    fallbacks: tuple[NativeSemanticFallback, ...]


def run_semantic_batch(payload: str, *, strict: bool = True) -> NativeSemanticBatchResult | None:
    """Run the native semantic batch engine for an already-normalized graph payload."""
    command = _native_command(strict=strict)
    if command is None:
        if strict:
            raise NativeSemanticBatchUnavailable("native semantic batch command is unavailable")
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
            raise NativeSemanticBatchUnavailable(f"native semantic batch failed: {stderr or exc}") from exc
        return None

    return _decode_result(completed.stdout)


def _native_command(*, strict: bool) -> list[str] | None:
    configured = os.environ.get("CODEBASE_GRAPH_COMPAT_SEMANTIC_BATCH")
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


def _decode_result(output: str) -> NativeSemanticBatchResult:
    symbol_count: int | None = None
    call_type_relations: int | None = None
    edges: list[NativeSemanticEdge] = []
    evidence: list[NativeSemanticEvidence] = []
    evidence_links: list[NativeSemanticEvidenceLink] = []
    fallbacks: list[NativeSemanticFallback] = []

    for line in output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        match parts[0]:
            case "RESULT" if len(parts) == 3:
                symbol_count = int(parts[1])
                call_type_relations = int(parts[2])
            case "EDGE" if len(parts) == 9:
                edges.append(
                    NativeSemanticEdge(
                        graph_index=int(parts[1]),
                        id=_unhex(parts[2]),
                        type=_unhex(parts[3]),
                        source_id=_unhex(parts[4]),
                        target_id=_unhex(parts[5]),
                        kind=_unhex(parts[6]),
                        confidence=float(parts[7]),
                        metadata=_json_dict(parts[8]),
                    )
                )
            case "EVIDENCE" if len(parts) == 7:
                evidence.append(
                    NativeSemanticEvidence(
                        evidence_id=_unhex(parts[1]),
                        source=_unhex(parts[2]),
                        confidence=float(parts[3]),
                        provider=_unhex(parts[4]),
                        diagnostics=tuple(str(item) for item in _json_list(parts[5])),
                        metadata=_json_dict(parts[6]),
                    )
                )
            case "LINK" if len(parts) == 7:
                evidence_links.append(
                    NativeSemanticEvidenceLink(
                        graph_index=int(parts[1]),
                        semantic_relation_id=_unhex(parts[2]),
                        evidence_node_id=_unhex(parts[3]),
                        evidence_kind=_unhex(parts[4]),
                        confidence=float(parts[5]),
                        metadata_fallback=parts[6] == "1",
                    )
                )
            case "FALLBACK" if len(parts) == 6:
                fallbacks.append(
                    NativeSemanticFallback(
                        graph_index=int(parts[1]),
                        semantic_relation_id=_unhex(parts[2]),
                        source_node_id=_unhex(parts[3]),
                        evidence_id=_unhex(parts[4]),
                        metadata=_json_dict(parts[5]),
                    )
                )
            case other:
                raise ValueError(f"Unknown native semantic batch record: {other}")

    if symbol_count is None or call_type_relations is None:
        raise ValueError("Native semantic batch did not report result counts")

    return NativeSemanticBatchResult(
        symbol_count=symbol_count,
        call_type_relations=call_type_relations,
        edges=tuple(edges),
        evidence=tuple(evidence),
        evidence_links=tuple(evidence_links),
        fallbacks=tuple(fallbacks),
    )


def _json_dict(value: str) -> dict[str, Any]:
    payload = json.loads(_unhex(value))
    if not isinstance(payload, dict):
        raise ValueError("Native semantic metadata must be a JSON object")
    return payload


def _json_list(value: str) -> list[Any]:
    payload = json.loads(_unhex(value))
    if not isinstance(payload, list):
        raise ValueError("Native semantic diagnostics must be a JSON array")
    return payload


def _unhex(value: str) -> str:
    return bytes.fromhex(value).decode("utf-8")
