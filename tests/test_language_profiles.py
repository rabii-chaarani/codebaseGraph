from __future__ import annotations

import json

import pytest

from codebase_graph.ingest import (
    CaptureMapping,
    LanguageCapability,
    LanguageProfile,
    load_language_profiles,
    profile_parser_version,
    register_language_support,
    resolve_language_profile,
    validate_language_profile,
)


def test_load_language_profiles_includes_required_languages() -> None:
    profiles = {profile.language: profile for profile in load_language_profiles()}

    assert {"rust", "go", "c", "cpp", "fortran"} <= set(profiles)
    assert ".rs" in profiles["rust"].suffixes
    assert profiles["rust"].capabilities.macros


def test_validate_language_profile_rejects_unknown_ontology_target() -> None:
    profile = LanguageProfile(
        language="zig",
        suffixes=(".zig",),
        grammar_package="tree_sitter_zig",
        root_node_types=("source_file",),
        capture_mappings=(
            CaptureMapping("definition.comptime", ("comptime_declaration",), "ComptimeBlock", ("Defines",)),
        ),
        capabilities=LanguageCapability(symbols=True),
    )

    with pytest.raises(KeyError):
        validate_language_profile(profile)


def test_resolve_language_profile_matches_language_or_suffix() -> None:
    assert resolve_language_profile("rust").language == "rust"
    assert resolve_language_profile("src/lib.rs").language == "rust"
    assert resolve_language_profile("src/main.cpp").language == "cpp"


def test_profile_parser_version_changes_with_profile_content() -> None:
    profile = resolve_language_profile("go")
    changed = LanguageProfile(
        language=profile.language,
        suffixes=profile.suffixes + (".gotmpl",),
        grammar_package=profile.grammar_package,
        root_node_types=profile.root_node_types,
        capture_mappings=profile.capture_mappings,
        capabilities=profile.capabilities,
    )

    assert profile_parser_version(profile) != profile_parser_version(changed)


def test_register_language_support_loads_repo_profiles(tmp_path) -> None:
    state = tmp_path / ".codebaseGraph"
    state.mkdir()
    (state / "language_profiles.json").write_text(
        json.dumps(
            [
                {
                    "language": "zig",
                    "suffixes": [".zig"],
                    "grammar_package": "tree_sitter_zig",
                    "root_node_types": ["source_file"],
                    "capture_mappings": [
                        {
                            "capture_name": "definition.function",
                            "parser_node_types": ["function_declaration"],
                            "target_node_type": "Function",
                            "relation_types": ["Defines"],
                        }
                    ],
                    "capabilities": {"symbols": True, "calls": True},
                }
            ]
        ),
        encoding="utf-8",
    )

    profiles = {profile.language: profile for profile in register_language_support(tmp_path)}

    assert "zig" in profiles
    assert profiles["zig"].parser_version.startswith("zig-profile-")
