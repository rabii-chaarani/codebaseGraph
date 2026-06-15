from __future__ import annotations

import json
from pathlib import Path

from codebase_graph.semantic import (
    collect_project_build_context,
    map_source_to_build_target,
    parse_c_family_build_context,
)


def test_collect_project_build_context_maps_supported_manifests(tmp_path: Path) -> None:
    (tmp_path / "Cargo.toml").write_text(
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n",
        encoding="utf-8",
    )
    (tmp_path / "go.mod").write_text("module example.com/demo\nrequire golang.org/x/text v0.1.0\n", encoding="utf-8")
    (tmp_path / "lib.rs").write_text("fn helper() {}\n", encoding="utf-8")
    (tmp_path / "main.go").write_text("package main\n", encoding="utf-8")

    context = collect_project_build_context(tmp_path, source_paths=("lib.rs", "main.go"))

    assert context.ecosystem == "mixed"
    assert {target.language for target in context.targets} >= {"rust", "go"}
    assert map_source_to_build_target(context, "lib.rs").name == "demo"  # type: ignore[union-attr]
    assert not [diagnostic for diagnostic in context.diagnostics if "lib.rs" in diagnostic]


def test_build_context_degrades_when_manifest_is_missing(tmp_path: Path) -> None:
    (tmp_path / "lib.rs").write_text("fn helper() {}\n", encoding="utf-8")

    context = collect_project_build_context(tmp_path, source_paths=("lib.rs",))

    assert context.targets
    assert map_source_to_build_target(context, "lib.rs") is not None
    assert context.diagnostics == ()


def test_parse_c_family_build_context_reads_compile_commands(tmp_path: Path) -> None:
    (tmp_path / "compile_commands.json").write_text(
        json.dumps(
            [
                {
                    "directory": tmp_path.as_posix(),
                    "file": (tmp_path / "lib.cpp").as_posix(),
                    "arguments": ["c++", "-Iinclude", "-c", "lib.cpp"],
                }
            ]
        ),
        encoding="utf-8",
    )
    (tmp_path / "lib.cpp").write_text("int helper() { return 1; }\n", encoding="utf-8")

    targets = parse_c_family_build_context(tmp_path, source_paths=("lib.cpp",))

    assert targets[0].language == "cpp"
    assert targets[0].source_paths == ("lib.cpp",)
    assert "-Iinclude" in targets[0].compiler_args
