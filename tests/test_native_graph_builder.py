from __future__ import annotations

import shutil
import subprocess
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any

import pytest

from codebase_graph._native import NativeGraphBuilderUnavailable
from codebase_graph._native.graph_builder import build_file_graph
from codebase_graph.extract import CaptureRecord, GraphBuilder, ParseBundle

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_MANIFEST = REPO_ROOT / "rust" / "Cargo.toml"
NATIVE_BINARY_NAME = "codebase_graph_native_graph_builder"
FIXTURE_ROOT = Path("tests/fixtures/golden_parity_project")


@pytest.fixture(scope="session")
def native_graph_builder_binary() -> Path:
    if shutil.which("cargo") is None:
        pytest.skip("cargo is required for native graph builder parity tests")

    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            RUST_MANIFEST.as_posix(),
            "--bin",
            NATIVE_BINARY_NAME,
            "--quiet",
        ],
        check=True,
    )
    suffix = ".exe" if sys.platform.startswith("win") else ""
    binary = REPO_ROOT / "rust" / "target" / "debug" / f"{NATIVE_BINARY_NAME}{suffix}"
    assert binary.exists()
    return binary


def test_native_graph_builder_matches_python_for_golden_capture_bundles(
    monkeypatch: pytest.MonkeyPatch,
    native_graph_builder_binary: Path,
) -> None:
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_GRAPH_BUILDER", native_graph_builder_binary.as_posix())

    for bundle in _golden_capture_bundles():
        expected = _python_graph(bundle)
        actual = build_file_graph(bundle, strict=True).graph.as_dict()

        assert actual == expected, f"native graph builder changed rows for {bundle.path}"


def test_native_graph_builder_uses_python_fallback_when_disabled(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE_GRAPH_BUILDER", raising=False)
    bundle = ParseBundle(
        language="python",
        path=(FIXTURE_ROOT / "src/app.py").as_posix(),
        source_root=".",
        repository_label="repo",
        captures=(
            _capture("definition.function", "function_definition", "handler", "def handler", 1, 2, 0, 11),
            _capture("reference.call", "call", "json_response", "json_response", 2, 2, 16, 29),
        ),
    )

    assert build_file_graph(bundle).graph.as_dict() == _python_graph(bundle)


def test_native_graph_builder_strict_mode_rejects_parse_tree_only_bundle(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("CODEBASE_GRAPH_NATIVE", raising=False)
    bundle = ParseBundle(
        language="python",
        path="app.py",
        source_root=".",
        repository_label="repo",
        tree={"type": "Module", "body": []},
    )

    with pytest.raises(NativeGraphBuilderUnavailable, match="only supports captures"):
        build_file_graph(bundle, strict=True)

    assert build_file_graph(bundle).graph.as_dict() == _python_graph(bundle)


def _python_graph(bundle: ParseBundle) -> dict[str, Any]:
    return (
        GraphBuilder(repository_label=bundle.repository_label, source_root=bundle.source_root)
        .build_file_graph(bundle)
        .graph.as_dict()
    )


def _golden_capture_bundles() -> Iterable[ParseBundle]:
    for language, path, captures in (
        (
            "python",
            "src/app.py",
            (
                _capture("reference.import", "import_statement", "util.helper", "from util import helper", 1, 1, 0, 23),
                _capture("definition.class", "class_definition", "AppService", "class AppService", 4, 6, 25, 41),
                _capture(
                    "definition.function", "function_definition", "helper_entry", "def helper_entry", 8, 9, 80, 96
                ),
                _capture("reference.call", "call", "helper", "helper", 9, 9, 100, 106),
            ),
        ),
        (
            "rust",
            "rust/lib.rs",
            (
                _capture("reference.import", "use_declaration", "std::fmt", "use std::fmt;", 1, 1, 0, 13),
                _capture("definition.struct", "struct_item", "Service", "struct Service", 3, 5, 14, 28),
                _capture("definition.method", "function_item", "new", "fn new", 8, 10, 45, 51),
                _capture("definition.function", "function_item", "helper", "fn helper", 13, 15, 80, 89),
                _capture("reference.call", "call_expression", "Service::new", "Service::new", 14, 14, 95, 107),
            ),
        ),
        (
            "go",
            "go/main.go",
            (
                _capture("reference.import", "import_spec", "fmt", '"fmt"', 3, 3, 14, 19),
                _capture("definition.class", "type_declaration", "Service", "type Service struct", 5, 7, 21, 40),
                _capture("definition.method", "method_declaration", "Run", "func (s Service) Run", 9, 11, 50, 70),
                _capture("definition.function", "function_declaration", "helper", "func helper", 13, 15, 90, 101),
                _capture("reference.call", "call_expression", "fmt.Println", "fmt.Println", 10, 10, 75, 86),
            ),
        ),
        (
            "c",
            "native/lib.c",
            (
                _capture("reference.include", "preproc_include", "stdio.h", "#include <stdio.h>", 1, 1, 0, 18),
                _capture("definition.struct", "struct_specifier", "Service", "struct Service", 3, 5, 20, 34),
                _capture("definition.function", "function_definition", "helper", "void helper", 7, 9, 60, 71),
                _capture("reference.call", "call_expression", "printf", "printf", 8, 8, 80, 86),
                _capture("definition.macro", "preproc_def", "SIZE", "#define SIZE 1", 11, 11, 100, 114),
            ),
        ),
        (
            "cpp",
            "native/lib.cpp",
            (
                _capture("reference.include", "preproc_include", "string", "#include <string>", 1, 1, 0, 17),
                _capture("definition.namespace", "namespace_definition", "app", "namespace app", 3, 12, 18, 31),
                _capture("definition.class", "class_specifier", "Service", "class Service", 4, 6, 35, 48),
                _capture("definition.method", "function_definition", "run", "Service::run", 8, 10, 70, 82),
                _capture("definition.function", "function_definition", "helper", "void helper", 12, 14, 100, 111),
                _capture("reference.call", "call_expression", "service.run", "service.run", 13, 13, 120, 131),
            ),
        ),
        (
            "fortran",
            "fortran/solver.f90",
            (
                _capture("definition.module", "module", "solver", "module solver", 1, 12, 0, 13),
                _capture("reference.import", "use_statement", "iso_fortran_env", "use iso_fortran_env", 2, 2, 14, 33),
                _capture("definition.function", "subroutine", "solve", "subroutine solve", 4, 7, 50, 66),
                _capture("definition.function", "function", "scale", "function scale", 9, 11, 90, 104),
                _capture("reference.call", "call_statement", "print_result", "print_result", 6, 6, 75, 87),
            ),
        ),
        (
            "markdown",
            "README.md",
            (
                _capture("doc.source", "document", "README", "Golden parity project", 1, 3, 0, 22),
                _capture("doc.section", "atx_heading", "Usage", "Usage", 5, 5, 30, 35),
            ),
        ),
    ):
        yield ParseBundle(
            language=language,
            path=(FIXTURE_ROOT / path).as_posix(),
            source_root=".",
            repository_label="repo",
            captures=captures,
        )


def _capture(
    capture_name: str,
    node_type: str,
    name: str,
    text: str,
    line_start: int,
    line_end: int,
    byte_start: int,
    byte_end: int,
) -> CaptureRecord:
    return CaptureRecord(
        capture_name,
        {
            "type": node_type,
            "name": name,
            "text": text,
            "line_start": line_start,
            "line_end": line_end,
            "byte_start": byte_start,
            "byte_end": byte_end,
        },
    )
