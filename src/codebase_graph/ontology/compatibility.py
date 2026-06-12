from __future__ import annotations

from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

from .ontology import get_node_type, relation_type_names


@dataclass(frozen=True, slots=True)
class SemanticConstructMapping:
    """Map one language construct onto the stable ontology."""

    language: str
    construct: str
    canonical_node_type: str
    kind: str
    metadata_schema: Mapping[str, str] = field(default_factory=dict)
    relation_types: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize this mapping for diagnostics and schema-facing payloads."""
        return {
            "language": self.language,
            "construct": self.construct,
            "canonical_node_type": self.canonical_node_type,
            "kind": self.kind,
            "metadata_schema": dict(self.metadata_schema),
            "relation_types": list(self.relation_types),
        }


@dataclass(frozen=True, slots=True)
class OntologyExtensionDecision:
    """Record whether a construct should reuse or extend the ontology."""

    construct: str
    action: str
    rationale: str
    migration_required: bool = False

    def as_dict(self) -> dict[str, Any]:
        """Serialize this decision for tooling and tests."""
        return {
            "construct": self.construct,
            "action": self.action,
            "rationale": self.rationale,
            "migration_required": self.migration_required,
        }


COMMON_METADATA_SCHEMA = {
    "language": "Source language that produced the construct.",
    "construct": "Language-specific construct name.",
}

DEFAULT_SEMANTIC_CONSTRUCT_MAPPINGS = (
    SemanticConstructMapping("rust", "struct", "Class", "struct", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("rust", "enum", "Class", "enum", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("rust", "trait", "Class", "trait", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("rust", "impl", "Scope", "impl", COMMON_METADATA_SCHEMA, ("Contains",)),
    SemanticConstructMapping("rust", "macro_definition", "Symbol", "macro", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("rust", "macro_invocation", "CallExpression", "macro_call", COMMON_METADATA_SCHEMA, ("Calls",)),
    SemanticConstructMapping("go", "struct_type", "Class", "struct", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("go", "interface_type", "Class", "interface", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("go", "method_declaration", "Method", "method", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("go", "package_clause", "Module", "package", COMMON_METADATA_SCHEMA, ("Contains",)),
    SemanticConstructMapping("c", "function_definition", "Function", "function", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("c", "struct_specifier", "Class", "struct", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("c", "union_specifier", "Class", "union", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("c", "enum_specifier", "Class", "enum", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("c", "preproc_include", "ImportDeclaration", "include", COMMON_METADATA_SCHEMA, ("Imports",)),
    SemanticConstructMapping("c", "preproc_def", "Symbol", "macro", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("cpp", "class_specifier", "Class", "class", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("cpp", "struct_specifier", "Class", "struct", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("cpp", "namespace_definition", "Module", "namespace", COMMON_METADATA_SCHEMA, ("Contains",)),
    SemanticConstructMapping("cpp", "template_declaration", "TypeAlias", "template", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("cpp", "preproc_include", "ImportDeclaration", "include", COMMON_METADATA_SCHEMA, ("Imports",)),
    SemanticConstructMapping("fortran", "module", "Module", "module", COMMON_METADATA_SCHEMA, ("Contains",)),
    SemanticConstructMapping("fortran", "subroutine", "Function", "subroutine", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("fortran", "function", "Function", "function", COMMON_METADATA_SCHEMA, ("Defines",)),
    SemanticConstructMapping("fortran", "use_statement", "ImportDeclaration", "use", COMMON_METADATA_SCHEMA, ("Imports",)),
    SemanticConstructMapping("framework", "route", "Route", "route", COMMON_METADATA_SCHEMA, ("RoutesTo", "Exposes")),
    SemanticConstructMapping(
        "framework",
        "endpoint",
        "APIEndpoint",
        "endpoint",
        COMMON_METADATA_SCHEMA,
        ("RoutesTo", "Exposes"),
    ),
    SemanticConstructMapping("framework", "component", "Component", "component", COMMON_METADATA_SCHEMA, ("Defines",)),
)

SCHEMA_EXTENSION_CONSTRUCTS = {
    "macro": "Macro semantics are cross-language, but reuse Symbol and CallExpression until macro analysis needs a table.",
    "namespace": "Namespace semantics are represented by Module or Scope until queries need a separate table.",
    "trait": "Trait semantics are represented by Class with kind='trait' for cross-language type containers.",
}


def map_semantic_construct(
    language: str,
    construct: str,
    mappings: Sequence[SemanticConstructMapping] = DEFAULT_SEMANTIC_CONSTRUCT_MAPPINGS,
) -> SemanticConstructMapping:
    """Choose the canonical ontology mapping for a language construct."""
    normalized_language = language.lower().strip()
    normalized_construct = _normalize_construct(construct)
    for mapping in mappings:
        if mapping.language == normalized_language and mapping.construct == normalized_construct:
            return mapping
    for mapping in mappings:
        if mapping.language == "framework" and mapping.construct == normalized_construct:
            return mapping
    raise KeyError(f"No ontology mapping for {language}:{construct}")


def validate_canonical_mapping(mapping: SemanticConstructMapping) -> SemanticConstructMapping:
    """Reject mappings that target undeclared ontology nodes or relations."""
    try:
        get_node_type(mapping.canonical_node_type)
    except KeyError as exc:
        raise ValueError(f"Unknown ontology node type: {mapping.canonical_node_type}") from exc
    relations = set(relation_type_names())
    unknown_relations = sorted(set(mapping.relation_types) - relations)
    if unknown_relations:
        joined = ", ".join(unknown_relations)
        raise ValueError(f"Unknown ontology relation(s) for {mapping.construct}: {joined}")
    for key in mapping.metadata_schema:
        if not key or not key.replace("_", "").isalnum() or key[0].isdigit():
            raise ValueError(f"Invalid metadata schema key for {mapping.construct}: {key}")
    return mapping


def decide_ontology_extension(construct: str, *, mapped_node_type: str | None = None) -> OntologyExtensionDecision:
    """Determine whether a construct should reuse the ontology or trigger a schema change."""
    normalized_construct = _normalize_construct(construct)
    if mapped_node_type:
        return OntologyExtensionDecision(
            construct=normalized_construct,
            action="reuse existing node",
            rationale=f"Represented by {mapped_node_type} with construct-specific kind and metadata.",
            migration_required=False,
        )
    rationale = SCHEMA_EXTENSION_CONSTRUCTS.get(
        normalized_construct,
        "Unknown construct should be modeled through a profile mapping before schema expansion.",
    )
    return OntologyExtensionDecision(
        construct=normalized_construct,
        action="defer schema change",
        rationale=rationale,
        migration_required=False,
    )


def evolve_cross_language_ontology(
    constructs: Iterable[tuple[str, str]],
    mappings: Sequence[SemanticConstructMapping] = DEFAULT_SEMANTIC_CONSTRUCT_MAPPINGS,
) -> tuple[OntologyExtensionDecision, ...]:
    """Evaluate constructs and return stable ontology-extension decisions."""
    decisions: list[OntologyExtensionDecision] = []
    for language, construct in constructs:
        try:
            mapping = validate_canonical_mapping(map_semantic_construct(language, construct, mappings))
        except KeyError:
            decisions.append(decide_ontology_extension(construct))
            continue
        decisions.append(decide_ontology_extension(construct, mapped_node_type=mapping.canonical_node_type))
    return tuple(decisions)


def _normalize_construct(construct: str) -> str:
    return construct.lower().strip().replace("-", "_").replace(" ", "_")
