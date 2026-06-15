from __future__ import annotations

import json
import shutil
import subprocess
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, Literal

from .build_context import BuildContext

ProviderMode = Literal["local_only", "opportunistic", "provider_first"]


@dataclass(frozen=True, slots=True)
class SemanticProvider:
    """Optional language tool that can answer semantic lookup requests."""

    name: str
    language: str
    executable: str
    capabilities: tuple[str, ...] = ()
    available: bool = False
    version: str = ""


@dataclass(frozen=True, slots=True)
class ProviderRequest:
    """Normalized semantic query sent to a provider."""

    provider: str
    language: str
    source_path: str
    position: tuple[int, int] | None
    query_kind: str


@dataclass(frozen=True, slots=True)
class ProviderResult:
    """Normalized provider answer with confidence and diagnostics."""

    provider: str
    query_kind: str
    target_symbol: str = ""
    confidence: float = 0.0
    diagnostics: tuple[str, ...] = ()
    metadata: Mapping[str, Any] = field(default_factory=dict)


def discover_semantic_providers(build_context: BuildContext | None = None) -> tuple[SemanticProvider, ...]:
    """Discover available language providers without executing them."""
    del build_context
    provider_specs = (
        ("rust_analyzer", "rust", "rust-analyzer", ("definition", "references")),
        ("gopls", "go", "gopls", ("definition", "references")),
        ("clangd", "c", "clangd", ("definition", "references")),
        ("clangd", "cpp", "clangd", ("definition", "references")),
        ("fortls", "fortran", "fortls", ("definition", "references")),
    )
    providers: list[SemanticProvider] = []
    for name, language, executable, capabilities in provider_specs:
        resolved = shutil.which(executable)
        providers.append(
            SemanticProvider(
                name=name,
                language=language,
                executable=resolved or executable,
                capabilities=capabilities,
                available=resolved is not None,
            )
        )
    return tuple(providers)


def select_semantic_provider(
    language: str,
    query_kind: str,
    providers: Sequence[SemanticProvider],
) -> SemanticProvider | None:
    """Select the best provider for a language and semantic query kind."""
    for provider in providers:
        if provider.language == language and query_kind in provider.capabilities and provider.available:
            return provider
    return None


def run_provider_query(
    request: ProviderRequest,
    providers: Sequence[SemanticProvider],
    *,
    provider_mode: ProviderMode = "local_only",
    timeout_seconds: float = 2.0,
) -> ProviderResult:
    """Run a bounded provider query when provider-backed resolution is enabled."""
    if provider_mode == "local_only":
        return ProviderResult(
            provider=request.provider,
            query_kind=request.query_kind,
            diagnostics=("provider execution disabled by local_only mode",),
        )
    provider = next((item for item in providers if item.name == request.provider and item.language == request.language), None)
    if provider is None or not provider.available:
        return ProviderResult(
            provider=request.provider,
            query_kind=request.query_kind,
            diagnostics=("semantic provider unavailable",),
        )
    try:
        completed = subprocess.run(
            [provider.executable, "--version"],
            check=False,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return ProviderResult(
            provider=provider.name,
            query_kind=request.query_kind,
            diagnostics=(f"provider query failed: {exc}",),
        )
    return normalize_provider_result(
        provider.name,
        request.query_kind,
        {"stdout": completed.stdout, "stderr": completed.stderr, "returncode": completed.returncode},
    )


def normalize_provider_result(provider: str, query_kind: str, payload: Mapping[str, Any] | str) -> ProviderResult:
    """Normalize provider-specific output into ProviderResult records."""
    if isinstance(payload, str):
        try:
            data: Mapping[str, Any] = json.loads(payload)
        except json.JSONDecodeError:
            data = {"stdout": payload}
    else:
        data = payload
    target = str(data.get("target_symbol") or data.get("symbol") or "").strip()
    diagnostics = tuple(str(item) for item in data.get("diagnostics", ()) if str(item))
    if not target and data.get("stderr"):
        diagnostics = (*diagnostics, str(data["stderr"]).strip())
    return ProviderResult(
        provider=provider,
        query_kind=query_kind,
        target_symbol=target,
        confidence=0.95 if target else 0.0,
        diagnostics=tuple(item for item in diagnostics if item),
        metadata=dict(data),
    )


def execute_provider_enrichment(
    requests: Sequence[ProviderRequest],
    *,
    build_context: BuildContext | None = None,
    provider_mode: ProviderMode = "local_only",
) -> tuple[ProviderResult, ...]:
    """Discover providers, execute semantic queries, and normalize provider evidence."""
    providers = discover_semantic_providers(build_context)
    return tuple(
        run_provider_query(request, providers, provider_mode=provider_mode)
        for request in requests
    )
