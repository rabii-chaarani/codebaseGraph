from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from codebase_graph.ontology import CONTEXT_PROFILES, relation_type_names


@dataclass(frozen=True, slots=True)
class ContextProfileSpec:
    """Normalized context-profile configuration."""
    description: str
    relations: tuple[str, ...]
    max_depth: int
    source: str = "builtin"

    def as_dict(self) -> dict[str, Any]:
        """Serialize a profile spec for runtime catalogs and schema payloads."""
        return {
            "description": self.description,
            "relations": list(self.relations),
            "max_depth": self.max_depth,
            "source": self.source,
        }


def builtin_context_profiles() -> dict[str, dict[str, Any]]:
    """Return built-in context profiles tagged with their source."""
    return {
        name: _normalize_profile(name, profile, source="builtin", relation_names=set(relation_type_names())).as_dict()
        for name, profile in CONTEXT_PROFILES.items()
    }


def load_context_profile_config(setup_payload: Mapping[str, Any]) -> dict[str, Any]:
    """Extract optional repo-defined context profiles from setup config."""
    raw_profiles = setup_payload.get("context_profiles", {})
    if raw_profiles in (None, {}):
        return {}
    if not isinstance(raw_profiles, Mapping):
        raise ValueError("context_profiles must be an object keyed by profile name")
    return {str(name): profile for name, profile in raw_profiles.items()}


def merge_context_profiles(custom_profiles: Mapping[str, Any] | None = None) -> dict[str, dict[str, Any]]:
    """Merge built-ins with repo-defined profiles after validation."""
    relation_names = set(relation_type_names())
    catalog = builtin_context_profiles()
    for name, profile in (custom_profiles or {}).items():
        if name in catalog:
            raise ValueError(f"Custom context profile {name!r} conflicts with a built-in profile")
        catalog[str(name)] = _normalize_profile(str(name), profile, source="repo", relation_names=relation_names).as_dict()
    return catalog


def validate_context_profile(name: str, profile: Mapping[str, Any]) -> dict[str, Any]:
    """Validate one profile and return its normalized dictionary form."""
    return _normalize_profile(name, profile, source=str(profile.get("source", "repo"))).as_dict()


def _normalize_profile(
    name: str,
    profile: Any,
    *,
    source: str,
    relation_names: set[str] | None = None,
) -> ContextProfileSpec:
    """Normalize and validate a profile mapping."""
    if not isinstance(profile, Mapping):
        raise ValueError(f"Context profile {name!r} must be an object")
    description = str(profile.get("description", "")).strip()
    if not description:
        raise ValueError(f"Context profile {name!r} requires a non-empty description")
    raw_relations = profile.get("relations", ())
    if not isinstance(raw_relations, (list, tuple)):
        raise ValueError(f"Context profile {name!r} relations must be a list")
    relations = tuple(str(relation).strip() for relation in raw_relations if str(relation).strip())
    if not relations:
        raise ValueError(f"Context profile {name!r} requires at least one relation")
    valid_relations = relation_names if relation_names is not None else set(relation_type_names())
    unknown = sorted(set(relations) - valid_relations)
    if unknown:
        joined = ", ".join(unknown)
        raise ValueError(f"Context profile {name!r} references unknown relation(s): {joined}")
    try:
        max_depth = int(profile.get("max_depth", 1))
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Context profile {name!r} max_depth must be an integer") from exc
    if max_depth <= 0:
        raise ValueError(f"Context profile {name!r} max_depth must be greater than zero")
    return ContextProfileSpec(description=description, relations=relations, max_depth=max_depth, source=source)


__all__ = [
    "ContextProfileSpec",
    "builtin_context_profiles",
    "load_context_profile_config",
    "merge_context_profiles",
    "validate_context_profile",
]
