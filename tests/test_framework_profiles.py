from __future__ import annotations

import json

from codebase_graph.extract import CaptureRecord
from codebase_graph.ingest import (
    DependencyEvidence,
    derive_framework_semantics,
    detect_framework_dependencies,
    emit_runtime_captures,
    load_framework_profiles,
    match_framework_markers,
)


def test_load_framework_profiles_includes_common_frameworks() -> None:
    profiles = {profile.name: profile for profile in load_framework_profiles()}

    assert {"fastapi", "express", "gin", "axum"} <= set(profiles)
    assert profiles["fastapi"].runtime_surfaces[0].target_node_type == "Route"


def test_detect_framework_dependencies_matches_manifest_evidence() -> None:
    evidence = [
        DependencyEvidence(
            ecosystem="cargo",
            name="axum",
            version="0.8",
            source_path="Cargo.toml",
            source_manifest="Cargo.toml",
        ),
        DependencyEvidence(
            ecosystem="pypi",
            name="requests",
            version="2",
            source_path="pyproject.toml",
            source_manifest="pyproject.toml",
        ),
    ]

    matches = detect_framework_dependencies(evidence)

    assert [(item.framework, item.source_kind, item.marker) for item in matches] == [
        ("axum", "dependency", "axum")
    ]


def test_match_framework_markers_matches_imports_and_calls() -> None:
    markers = [
        CaptureRecord("reference.import", {"text": "fastapi.APIRouter", "path": "api.py", "line_start": 1}),
        CaptureRecord("decorator", {"text": "@router.get", "path": "api.py", "line_start": 5}),
    ]

    matches = match_framework_markers(markers)

    assert {item.framework for item in matches} == {"fastapi"}
    assert {item.source_kind for item in matches} == {"import", "decorator"}


def test_emit_runtime_captures_turns_framework_evidence_into_route_captures() -> None:
    evidence = match_framework_markers(
        [CaptureRecord("reference.call", {"text": "app.get", "path": "server.ts", "line_start": 7})]
    )

    captures = emit_runtime_captures(evidence)

    assert {capture.capture for capture in captures} >= {"route", "endpoint"}
    assert {capture.node["framework"] for capture in captures} == {"express"}


def test_derive_framework_semantics_combines_dependency_and_source_evidence() -> None:
    captures = derive_framework_semantics(
        dependency_evidence=[
            DependencyEvidence(
                ecosystem="npm",
                name="next",
                version="16",
                source_path="package.json",
                source_manifest="package.json",
            )
        ],
        source_markers=[CaptureRecord("reference.import", {"text": "next/navigation"})],
    )

    assert {capture.node["framework"] for capture in captures} == {"next"}


def test_load_framework_profiles_reads_repo_config(tmp_path) -> None:
    state = tmp_path / ".codebaseGraph"
    state.mkdir()
    (state / "framework_profiles.json").write_text(
        json.dumps(
            [
                {
                    "name": "custom",
                    "ecosystems": ["npm"],
                    "dependency_markers": ["custom"],
                    "runtime_surfaces": [{"capture_name": "route", "target_node_type": "Route"}],
                }
            ]
        ),
        encoding="utf-8",
    )

    profiles = {profile.name: profile for profile in load_framework_profiles(tmp_path)}

    assert "custom" in profiles
