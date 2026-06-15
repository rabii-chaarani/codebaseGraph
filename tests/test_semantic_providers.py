from __future__ import annotations

import subprocess

from codebase_graph.semantic import (
    ProviderRequest,
    SemanticProvider,
    normalize_provider_result,
    run_provider_query,
    select_semantic_provider,
)


def test_local_only_provider_mode_does_not_execute_provider(monkeypatch) -> None:
    def fail_run(*args, **kwargs):  # noqa: ANN001, ANN002, ANN003
        raise AssertionError("local_only mode must not execute provider processes")

    monkeypatch.setattr(subprocess, "run", fail_run)
    provider = SemanticProvider(
        name="rust_analyzer",
        language="rust",
        executable="rust-analyzer",
        capabilities=("definition",),
        available=True,
    )

    result = run_provider_query(
        ProviderRequest("rust_analyzer", "rust", "lib.rs", (1, 1), "definition"),
        (provider,),
        provider_mode="local_only",
    )

    assert result.confidence == 0.0
    assert result.diagnostics == ("provider execution disabled by local_only mode",)


def test_select_semantic_provider_matches_language_and_capability() -> None:
    provider = SemanticProvider("gopls", "go", "gopls", ("definition",), available=True)

    assert select_semantic_provider("go", "definition", (provider,)) == provider
    assert select_semantic_provider("go", "references", (provider,)) is None


def test_normalize_provider_result_accepts_mapping_payload() -> None:
    result = normalize_provider_result("gopls", "definition", {"target_symbol": "helper"})

    assert result.target_symbol == "helper"
    assert result.confidence == 0.95
