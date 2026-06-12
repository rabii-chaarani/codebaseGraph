from __future__ import annotations

from pathlib import Path

from codebase_graph.core import CodeGraph, GraphNode
from codebase_graph.ingest import (
    classify_dependency_ecosystem,
    detect_dependency_manifest,
    enrich_dependency_context,
    link_dependency_evidence,
    parse_dependency_manifest,
)


def test_detect_dependency_manifest_finds_supported_manifests(tmp_path: Path) -> None:
    (tmp_path / "pyproject.toml").write_text("[project]\ndependencies = ['requests>=2']\n", encoding="utf-8")
    nested = tmp_path / "service"
    nested.mkdir()
    (nested / "go.mod").write_text("module example\nrequire github.com/acme/lib v1.2.3\n", encoding="utf-8")
    ignored = tmp_path / "node_modules"
    ignored.mkdir()
    (ignored / "package.json").write_text('{"dependencies":{"ignored":"1"}}', encoding="utf-8")

    manifests = [path.relative_to(tmp_path).as_posix() for path in detect_dependency_manifest(tmp_path)]

    assert manifests == ["pyproject.toml", "service/go.mod"]


def test_parse_dependency_manifest_extracts_python_and_go_dependencies(tmp_path: Path) -> None:
    pyproject = tmp_path / "pyproject.toml"
    pyproject.write_text(
        "[build-system]\nrequires = ['setuptools>=77']\n"
        "[project]\ndependencies = ['requests>=2', 'fastapi[standard]==0.1']\n",
        encoding="utf-8",
    )
    gomod = tmp_path / "go.mod"
    gomod.write_text("module example\nrequire (\n github.com/acme/lib v1.2.3\n)\n", encoding="utf-8")

    python_evidence = parse_dependency_manifest(pyproject, source_root=tmp_path)
    go_evidence = parse_dependency_manifest(gomod, source_root=tmp_path)

    assert {(item.ecosystem, item.name, item.version) for item in python_evidence} == {
        ("pypi", "setuptools", ">=77"),
        ("pypi", "requests", ">=2"),
        ("pypi", "fastapi", "==0.1"),
    }
    assert [(item.ecosystem, item.name, item.version) for item in go_evidence] == [
        ("go", "github.com/acme/lib", "v1.2.3")
    ]


def test_classify_dependency_ecosystem_covers_native_manifests() -> None:
    assert classify_dependency_ecosystem("Cargo.toml") == "cargo"
    assert classify_dependency_ecosystem("CMakeLists.txt") == "cmake"
    assert classify_dependency_ecosystem("fpm.toml") == "fortran"


def test_link_dependency_evidence_attaches_dependency_to_file_node(tmp_path: Path) -> None:
    pyproject = tmp_path / "pyproject.toml"
    pyproject.write_text("[project]\ndependencies = ['requests>=2']\n", encoding="utf-8")
    evidence = parse_dependency_manifest(pyproject, source_root=tmp_path)[0]
    graph = CodeGraph()
    graph.add_node(
        GraphNode(
            id="File:pyproject",
            table="File",
            label="pyproject.toml",
            kind="file",
            path="pyproject.toml",
        )
    )

    dependency = link_dependency_evidence(graph, evidence)

    assert dependency.table == "Dependency"
    assert dependency.metadata["ecosystem"] == "pypi"
    assert {edge.type for edge in graph.edges.values()} == {"DependsOn", "EvidencedBy"}


def test_enrich_dependency_context_parses_all_supported_manifests(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text('{"dependencies":{"react":"^19"}}', encoding="utf-8")
    (tmp_path / "Cargo.toml").write_text("[dependencies]\nserde = '1'\n", encoding="utf-8")

    evidence = enrich_dependency_context(tmp_path)

    assert {(item.ecosystem, item.name) for item in evidence} == {("cargo", "serde"), ("npm", "react")}
