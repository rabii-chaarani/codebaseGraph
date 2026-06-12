from __future__ import annotations

import hashlib
import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any

from codebase_graph.ontology import get_node_type, relation_type_names
from codebase_graph.ontology.compatibility import map_semantic_construct, validate_canonical_mapping


@dataclass(frozen=True, slots=True)
class LanguageCapability:
    """Capability flags that describe semantic depth for a language profile."""

    symbols: bool = False
    imports: bool = False
    calls: bool = False
    types: bool = False
    macros: bool = False
    runtime_surfaces: bool = False

    @classmethod
    def from_mapping(cls, payload: Mapping[str, Any]) -> LanguageCapability:
        """Create capabilities from a JSON-compatible mapping."""
        return cls(
            symbols=bool(payload.get("symbols", False)),
            imports=bool(payload.get("imports", False)),
            calls=bool(payload.get("calls", False)),
            types=bool(payload.get("types", False)),
            macros=bool(payload.get("macros", False)),
            runtime_surfaces=bool(payload.get("runtime_surfaces", False)),
        )

    def as_dict(self) -> dict[str, bool]:
        """Serialize capabilities into a deterministic mapping."""
        return {
            "symbols": self.symbols,
            "imports": self.imports,
            "calls": self.calls,
            "types": self.types,
            "macros": self.macros,
            "runtime_surfaces": self.runtime_surfaces,
        }


@dataclass(frozen=True, slots=True)
class CaptureMapping:
    """Map concrete parser nodes and captures onto canonical ontology targets."""

    capture_name: str
    parser_node_types: tuple[str, ...]
    target_node_type: str
    relation_types: tuple[str, ...] = ()
    context_rule: str = ""
    construct: str = ""

    @classmethod
    def from_mapping(cls, payload: Mapping[str, Any]) -> CaptureMapping:
        """Create a capture mapping from JSON-compatible data."""
        return cls(
            capture_name=str(payload["capture_name"]),
            parser_node_types=tuple(str(item) for item in payload.get("parser_node_types", ())),
            target_node_type=str(payload["target_node_type"]),
            relation_types=tuple(str(item) for item in payload.get("relation_types", ())),
            context_rule=str(payload.get("context_rule", "")),
            construct=str(payload.get("construct", "")),
        )

    def as_dict(self) -> dict[str, Any]:
        """Serialize this mapping into deterministic profile data."""
        return {
            "capture_name": self.capture_name,
            "parser_node_types": list(self.parser_node_types),
            "target_node_type": self.target_node_type,
            "relation_types": list(self.relation_types),
            "context_rule": self.context_rule,
            "construct": self.construct,
        }


@dataclass(frozen=True, slots=True)
class LanguageProfile:
    """Declarative parser profile for one supported source language."""

    language: str
    suffixes: tuple[str, ...]
    grammar_package: str
    root_node_types: tuple[str, ...]
    capture_mappings: tuple[CaptureMapping, ...]
    capabilities: LanguageCapability
    parser_version: str = ""

    @classmethod
    def from_mapping(cls, payload: Mapping[str, Any]) -> LanguageProfile:
        """Create a language profile from JSON-compatible data."""
        profile = cls(
            language=str(payload["language"]),
            suffixes=tuple(str(item) for item in payload.get("suffixes", ())),
            grammar_package=str(payload.get("grammar_package", "")),
            root_node_types=tuple(str(item) for item in payload.get("root_node_types", ())),
            capture_mappings=tuple(CaptureMapping.from_mapping(item) for item in payload.get("capture_mappings", ())),
            capabilities=LanguageCapability.from_mapping(payload.get("capabilities", {})),
            parser_version=str(payload.get("parser_version", "")),
        )
        return profile if profile.parser_version else replace(profile, parser_version=profile_parser_version(profile))

    def as_dict(self, *, include_parser_version: bool = True) -> dict[str, Any]:
        """Serialize the profile into deterministic data."""
        payload: dict[str, Any] = {
            "language": self.language,
            "suffixes": list(self.suffixes),
            "grammar_package": self.grammar_package,
            "root_node_types": list(self.root_node_types),
            "capture_mappings": [mapping.as_dict() for mapping in self.capture_mappings],
            "capabilities": self.capabilities.as_dict(),
        }
        if include_parser_version:
            payload["parser_version"] = self.parser_version
        return payload


def load_language_profiles(
    source_root: str | Path | None = None,
    *,
    extra_profiles: Sequence[LanguageProfile] = (),
) -> tuple[LanguageProfile, ...]:
    """Load built-in and optional repository language profile specs."""
    profiles = [_with_parser_version(profile) for profile in BUILTIN_LANGUAGE_PROFILES]
    if source_root is not None:
        profiles.extend(_load_repo_profiles(Path(source_root)))
    profiles.extend(_with_parser_version(profile) for profile in extra_profiles)
    return tuple(validate_language_profile(profile) for profile in _deduplicate_profiles(profiles))


def validate_language_profile(profile: LanguageProfile) -> LanguageProfile:
    """Validate profile shape, ontology targets, relations, and construct mappings."""
    if not profile.language.strip():
        raise ValueError("Language profile requires a language key")
    if not profile.suffixes:
        raise ValueError(f"Language profile {profile.language} requires at least one suffix")
    if not profile.root_node_types:
        raise ValueError(f"Language profile {profile.language} requires root node types")
    relations = set(relation_type_names())
    for mapping in profile.capture_mappings:
        if not mapping.capture_name:
            raise ValueError(f"Language profile {profile.language} has an empty capture name")
        if not mapping.parser_node_types:
            raise ValueError(f"Capture mapping {mapping.capture_name} requires parser node types")
        get_node_type(mapping.target_node_type)
        unknown_relations = sorted(set(mapping.relation_types) - relations)
        if unknown_relations:
            joined = ", ".join(unknown_relations)
            raise ValueError(f"Capture mapping {mapping.capture_name} has unknown relation(s): {joined}")
        if mapping.construct:
            construct_mapping = map_semantic_construct(profile.language, mapping.construct)
            validate_canonical_mapping(construct_mapping)
            if construct_mapping.canonical_node_type != mapping.target_node_type:
                raise ValueError(
                    f"{profile.language}:{mapping.construct} maps to "
                    f"{construct_mapping.canonical_node_type}, not {mapping.target_node_type}"
                )
    return profile


def resolve_language_profile(
    path_or_language: str | Path,
    profiles: Sequence[LanguageProfile] | None = None,
) -> LanguageProfile | None:
    """Select the matching LanguageProfile for a path or language key."""
    candidates = tuple(profiles) if profiles is not None else load_language_profiles()
    text = Path(path_or_language).as_posix() if isinstance(path_or_language, Path) else str(path_or_language)
    normalized = text.lower()
    for profile in candidates:
        if profile.language == normalized:
            return profile
    path = Path(text)
    suffixes = [path.suffix.lower()]
    if path.suffix.lower() == ".h":
        suffixes.append(".hpp")
    for profile in candidates:
        if any(suffix in profile.suffixes for suffix in suffixes):
            return profile
    return None


def profile_parser_version(profile: LanguageProfile) -> str:
    """Build deterministic parser-version fragments from profile content."""
    encoded = json.dumps(profile.as_dict(include_parser_version=False), sort_keys=True, separators=(",", ":"))
    digest = hashlib.sha1(encoded.encode("utf-8")).hexdigest()[:12]
    return f"{profile.language}-profile-{digest}"


def register_language_support(
    source_root: str | Path | None = None,
    *,
    extra_profiles: Sequence[LanguageProfile] = (),
) -> tuple[LanguageProfile, ...]:
    """Load, validate, resolve, and version language profiles before parser registration."""
    return load_language_profiles(source_root, extra_profiles=extra_profiles)


def _with_parser_version(profile: LanguageProfile) -> LanguageProfile:
    return profile if profile.parser_version else replace(profile, parser_version=profile_parser_version(profile))


def _load_repo_profiles(source_root: Path) -> list[LanguageProfile]:
    config_path = source_root / ".codebaseGraph" / "language_profiles.json"
    if not config_path.exists():
        return []
    payload = json.loads(config_path.read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError("language_profiles.json must contain a list of profiles")
    return [_with_parser_version(LanguageProfile.from_mapping(item)) for item in payload]


def _deduplicate_profiles(profiles: Sequence[LanguageProfile]) -> list[LanguageProfile]:
    by_language: dict[str, LanguageProfile] = {}
    for profile in profiles:
        by_language[profile.language] = profile
    return [by_language[key] for key in sorted(by_language)]


def _capture(
    capture_name: str,
    parser_node_types: Sequence[str],
    target_node_type: str,
    relation_types: Sequence[str],
    *,
    construct: str = "",
    context_rule: str = "",
) -> CaptureMapping:
    return CaptureMapping(
        capture_name=capture_name,
        parser_node_types=tuple(parser_node_types),
        target_node_type=target_node_type,
        relation_types=tuple(relation_types),
        construct=construct,
        context_rule=context_rule,
    )


def _profile(
    language: str,
    suffixes: Sequence[str],
    grammar_package: str,
    root_node_types: Sequence[str],
    capture_mappings: Sequence[CaptureMapping],
    capabilities: LanguageCapability,
) -> LanguageProfile:
    profile = LanguageProfile(
        language=language,
        suffixes=tuple(suffixes),
        grammar_package=grammar_package,
        root_node_types=tuple(root_node_types),
        capture_mappings=tuple(capture_mappings),
        capabilities=capabilities,
    )
    return _with_parser_version(profile)


BUILTIN_LANGUAGE_PROFILES = (
    _profile(
        "rust",
        (".rs",),
        "tree_sitter_rust",
        ("source_file",),
        (
            _capture("definition.struct", ("struct_item",), "Class", ("Defines",), construct="struct"),
            _capture("definition.enum", ("enum_item",), "Class", ("Defines",), construct="enum"),
            _capture("definition.interface", ("trait_item",), "Class", ("Defines",), construct="trait"),
            _capture("definition.method", ("function_item",), "Method", ("Defines",), context_rule="inside impl"),
            _capture("definition.method", ("function_signature_item",), "Method", ("Defines",)),
            _capture("definition.function", ("function_item",), "Function", ("Defines",)),
            _capture("definition.type_alias", ("type_item",), "TypeAlias", ("Defines",)),
            _capture("reference.use", ("use_declaration",), "ImportDeclaration", ("Imports",)),
            _capture("reference.call", ("call_expression",), "CallExpression", ("Calls",)),
            _capture("reference.macro", ("macro_invocation",), "CallExpression", ("Calls",), construct="macro_invocation"),
        ),
        LanguageCapability(symbols=True, imports=True, calls=True, types=True, macros=True, runtime_surfaces=True),
    ),
    _profile(
        "go",
        (".go",),
        "tree_sitter_go",
        ("source_file",),
        (
            _capture("definition.package", ("package_clause",), "Module", ("Contains",), construct="package_clause"),
            _capture(
                "definition.struct",
                ("type_declaration",),
                "Class",
                ("Defines",),
                construct="struct_type",
                context_rule="type is struct_type",
            ),
            _capture(
                "definition.interface",
                ("type_declaration",),
                "Class",
                ("Defines",),
                construct="interface_type",
                context_rule="type is interface_type",
            ),
            _capture("definition.function", ("function_declaration",), "Function", ("Defines",)),
            _capture("definition.method", ("method_declaration",), "Method", ("Defines",), construct="method_declaration"),
            _capture("reference.import", ("import_declaration",), "ImportDeclaration", ("Imports",)),
            _capture("reference.call", ("call_expression",), "CallExpression", ("Calls",)),
            _capture("type.annotation", ("type_identifier", "qualified_type"), "TypeAnnotation", ("HasTypeAnnotation",)),
        ),
        LanguageCapability(symbols=True, imports=True, calls=True, types=True, runtime_surfaces=True),
    ),
    _profile(
        "c",
        (".c", ".h"),
        "tree_sitter_c",
        ("translation_unit",),
        (
            _capture("definition.function", ("function_definition",), "Function", ("Defines",), construct="function_definition"),
            _capture("definition.struct", ("struct_specifier",), "Class", ("Defines",), construct="struct_specifier"),
            _capture("definition.union", ("union_specifier",), "Class", ("Defines",), construct="union_specifier"),
            _capture("definition.enum", ("enum_specifier",), "Class", ("Defines",), construct="enum_specifier"),
            _capture("reference.include", ("preproc_include",), "ImportDeclaration", ("Imports",), construct="preproc_include"),
            _capture("definition.macro", ("preproc_def",), "Symbol", ("Defines",), construct="preproc_def"),
            _capture("reference.call", ("call_expression",), "CallExpression", ("Calls",)),
        ),
        LanguageCapability(symbols=True, imports=True, calls=True, types=True, macros=True),
    ),
    _profile(
        "cpp",
        (".cc", ".cpp", ".cxx", ".hh", ".hpp", ".hxx"),
        "tree_sitter_cpp",
        ("translation_unit",),
        (
            _capture("definition.class", ("class_specifier",), "Class", ("Defines",), construct="class_specifier"),
            _capture("definition.struct", ("struct_specifier",), "Class", ("Defines",), construct="struct_specifier"),
            _capture("definition.namespace", ("namespace_definition",), "Module", ("Contains",), construct="namespace_definition"),
            _capture("definition.template", ("template_declaration",), "TypeAlias", ("Defines",), construct="template_declaration"),
            _capture(
                "definition.method",
                ("field_declaration",),
                "Method",
                ("Defines",),
                context_rule="function declarator",
            ),
            _capture(
                "definition.method",
                ("function_definition",),
                "Method",
                ("Defines",),
                context_rule="qualified declarator",
            ),
            _capture("definition.function", ("function_definition",), "Function", ("Defines",)),
            _capture("reference.include", ("preproc_include",), "ImportDeclaration", ("Imports",), construct="preproc_include"),
            _capture("reference.call", ("call_expression",), "CallExpression", ("Calls",)),
        ),
        LanguageCapability(symbols=True, imports=True, calls=True, types=True, macros=True, runtime_surfaces=True),
    ),
    _profile(
        "fortran",
        (".f", ".for", ".f90", ".f95", ".f03", ".f08"),
        "tree_sitter_fortran",
        ("translation_unit", "program"),
        (
            _capture("definition.module", ("module",), "Module", ("Contains",), construct="module"),
            _capture("definition.subroutine", ("subroutine",), "Function", ("Defines",), construct="subroutine"),
            _capture("definition.function", ("function",), "Function", ("Defines",), construct="function"),
            _capture("reference.use", ("use_statement",), "ImportDeclaration", ("Imports",), construct="use_statement"),
            _capture("reference.call", ("call_statement", "subroutine_call"), "CallExpression", ("Calls",)),
        ),
        LanguageCapability(symbols=True, imports=True, calls=True, types=True),
    ),
)
