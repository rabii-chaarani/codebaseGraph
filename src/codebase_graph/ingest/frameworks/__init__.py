from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.extract import CaptureRecord
from codebase_graph.ingest.manifest_detector import DependencyEvidence
from codebase_graph.ontology import get_node_type
from codebase_graph.ontology.compatibility import map_semantic_construct, validate_canonical_mapping


@dataclass(frozen=True, slots=True)
class RuntimeSurfaceRule:
    """Framework rule that emits route, endpoint, component, query, or secret captures."""

    capture_name: str
    target_node_type: str
    method_source: str = ""
    path_source: str = ""
    handler_source: str = ""

    @classmethod
    def from_mapping(cls, payload: Mapping[str, Any]) -> RuntimeSurfaceRule:
        """Create a runtime surface rule from JSON-compatible data."""
        return cls(
            capture_name=str(payload["capture_name"]),
            target_node_type=str(payload["target_node_type"]),
            method_source=str(payload.get("method_source", "")),
            path_source=str(payload.get("path_source", "")),
            handler_source=str(payload.get("handler_source", "")),
        )

    def as_dict(self) -> dict[str, str]:
        """Serialize rule data."""
        return {
            "capture_name": self.capture_name,
            "target_node_type": self.target_node_type,
            "method_source": self.method_source,
            "path_source": self.path_source,
            "handler_source": self.handler_source,
        }


@dataclass(frozen=True, slots=True)
class FrameworkProfile:
    """Declarative framework profile used to derive runtime and component semantics."""

    name: str
    ecosystems: tuple[str, ...]
    dependency_markers: tuple[str, ...]
    import_markers: tuple[str, ...] = ()
    decorator_markers: tuple[str, ...] = ()
    call_markers: tuple[str, ...] = ()
    runtime_surfaces: tuple[RuntimeSurfaceRule, ...] = ()

    @classmethod
    def from_mapping(cls, payload: Mapping[str, Any]) -> FrameworkProfile:
        """Create a framework profile from JSON-compatible data."""
        return cls(
            name=str(payload["name"]),
            ecosystems=tuple(str(item) for item in payload.get("ecosystems", ())),
            dependency_markers=tuple(str(item) for item in payload.get("dependency_markers", ())),
            import_markers=tuple(str(item) for item in payload.get("import_markers", ())),
            decorator_markers=tuple(str(item) for item in payload.get("decorator_markers", ())),
            call_markers=tuple(str(item) for item in payload.get("call_markers", ())),
            runtime_surfaces=tuple(
                RuntimeSurfaceRule.from_mapping(item) for item in payload.get("runtime_surfaces", ())
            ),
        )

    def as_dict(self) -> dict[str, Any]:
        """Serialize profile data."""
        return {
            "name": self.name,
            "ecosystems": list(self.ecosystems),
            "dependency_markers": list(self.dependency_markers),
            "import_markers": list(self.import_markers),
            "decorator_markers": list(self.decorator_markers),
            "call_markers": list(self.call_markers),
            "runtime_surfaces": [rule.as_dict() for rule in self.runtime_surfaces],
        }


@dataclass(frozen=True, slots=True)
class FrameworkEvidence:
    """Evidence item showing why a framework rule matched source or manifest data."""

    framework: str
    source_kind: str
    marker: str
    confidence: float
    path: str = ""
    span: tuple[int, int] | None = None

    def as_dict(self) -> dict[str, Any]:
        """Serialize evidence data."""
        return {
            "framework": self.framework,
            "source_kind": self.source_kind,
            "marker": self.marker,
            "confidence": self.confidence,
            "path": self.path,
            "span": list(self.span) if self.span is not None else None,
        }


def load_framework_profiles(source_root: str | Path | None = None) -> tuple[FrameworkProfile, ...]:
    """Load built-in and optional repository framework profiles."""
    profiles = list(BUILTIN_FRAMEWORK_PROFILES)
    if source_root is not None:
        profiles.extend(_load_repo_framework_profiles(Path(source_root)))
    return tuple(_validate_framework_profile(profile) for profile in _deduplicate_profiles(profiles))


def detect_framework_dependencies(
    dependency_evidence: Sequence[DependencyEvidence],
    profiles: Sequence[FrameworkProfile] | None = None,
) -> tuple[FrameworkEvidence, ...]:
    """Turn manifest dependency evidence into FrameworkEvidence values."""
    candidates = tuple(profiles) if profiles is not None else load_framework_profiles()
    evidence: list[FrameworkEvidence] = []
    for dependency in dependency_evidence:
        dependency_name = dependency.name.lower()
        for profile in candidates:
            if profile.ecosystems and dependency.ecosystem not in profile.ecosystems:
                continue
            if any(_marker_matches(dependency_name, marker) for marker in profile.dependency_markers):
                evidence.append(
                    FrameworkEvidence(
                        framework=profile.name,
                        source_kind="dependency",
                        marker=dependency.name,
                        confidence=dependency.confidence,
                        path=dependency.source_path,
                    )
                )
    return tuple(evidence)


def match_framework_markers(
    source_markers: Sequence[CaptureRecord | Mapping[str, Any] | str],
    profiles: Sequence[FrameworkProfile] | None = None,
) -> tuple[FrameworkEvidence, ...]:
    """Match imports, decorators, macros, calls, and config keys against framework profiles."""
    candidates = tuple(profiles) if profiles is not None else load_framework_profiles()
    evidence: list[FrameworkEvidence] = []
    for marker in source_markers:
        text = _marker_text(marker).lower()
        source_kind = _source_kind(marker)
        for profile in candidates:
            marker_groups = _profile_markers_for_kind(profile, source_kind)
            if any(_marker_matches(text, framework_marker) for framework_marker in marker_groups):
                evidence.append(
                    FrameworkEvidence(
                        framework=profile.name,
                        source_kind=source_kind,
                        marker=_marker_text(marker),
                        confidence=0.85,
                        path=_marker_path(marker),
                        span=_marker_span(marker),
                    )
                )
    return tuple(evidence)


def emit_runtime_captures(
    framework_evidence: Sequence[FrameworkEvidence],
    profiles: Sequence[FrameworkProfile] | None = None,
) -> tuple[CaptureRecord, ...]:
    """Convert matched framework rules into runtime-surface graph captures."""
    candidates = {profile.name: profile for profile in (profiles or load_framework_profiles())}
    captures: list[CaptureRecord] = []
    for evidence in framework_evidence:
        profile = candidates.get(evidence.framework)
        if profile is None:
            continue
        for rule in profile.runtime_surfaces:
            captures.append(
                CaptureRecord(
                    rule.capture_name,
                    {
                        "type": rule.target_node_type,
                        "capture_name": rule.capture_name,
                        "name": evidence.marker,
                        "text": evidence.marker,
                        "framework": evidence.framework,
                        "source_kind": evidence.source_kind,
                        "path": evidence.path,
                        "line_start": evidence.span[0] if evidence.span else None,
                        "line_end": evidence.span[1] if evidence.span else None,
                        "metadata": {"framework": evidence.framework, "rule": rule.as_dict()},
                    },
                )
            )
    return tuple(captures)


def derive_framework_semantics(
    *,
    dependency_evidence: Sequence[DependencyEvidence] = (),
    source_markers: Sequence[CaptureRecord | Mapping[str, Any] | str] = (),
    profiles: Sequence[FrameworkProfile] | None = None,
) -> tuple[CaptureRecord, ...]:
    """Combine dependency and source evidence, match rules, and emit runtime captures."""
    candidates = tuple(profiles) if profiles is not None else load_framework_profiles()
    framework_evidence = (
        *detect_framework_dependencies(dependency_evidence, candidates),
        *match_framework_markers(source_markers, candidates),
    )
    return emit_runtime_captures(framework_evidence, candidates)


def _validate_framework_profile(profile: FrameworkProfile) -> FrameworkProfile:
    if not profile.name:
        raise ValueError("Framework profile requires a name")
    for rule in profile.runtime_surfaces:
        get_node_type(rule.target_node_type)
        construct = _construct_for_target(rule.target_node_type)
        validate_canonical_mapping(map_semantic_construct("framework", construct))
    return profile


def _construct_for_target(target_node_type: str) -> str:
    if target_node_type == "Route":
        return "route"
    if target_node_type == "APIEndpoint":
        return "endpoint"
    if target_node_type == "Component":
        return "component"
    return target_node_type.lower()


def _load_repo_framework_profiles(source_root: Path) -> list[FrameworkProfile]:
    config_path = source_root / ".codebaseGraph" / "framework_profiles.json"
    if not config_path.exists():
        return []
    payload = json.loads(config_path.read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError("framework_profiles.json must contain a list of profiles")
    return [FrameworkProfile.from_mapping(item) for item in payload]


def _deduplicate_profiles(profiles: Sequence[FrameworkProfile]) -> list[FrameworkProfile]:
    by_name: dict[str, FrameworkProfile] = {}
    for profile in profiles:
        by_name[profile.name] = profile
    return [by_name[name] for name in sorted(by_name)]


def _profile_markers_for_kind(profile: FrameworkProfile, source_kind: str) -> tuple[str, ...]:
    if source_kind == "import":
        return profile.import_markers
    if source_kind == "decorator":
        return profile.decorator_markers
    if source_kind in {"call", "macro", "config"}:
        return profile.call_markers
    return (*profile.import_markers, *profile.decorator_markers, *profile.call_markers)


def _source_kind(marker: CaptureRecord | Mapping[str, Any] | str) -> str:
    capture = _marker_capture(marker)
    if "import" in capture or "include" in capture or "use" in capture:
        return "import"
    if "decorator" in capture:
        return "decorator"
    if "macro" in capture:
        return "macro"
    if "call" in capture:
        return "call"
    if "config" in capture:
        return "config"
    return "marker"


def _marker_capture(marker: CaptureRecord | Mapping[str, Any] | str) -> str:
    if isinstance(marker, CaptureRecord):
        return marker.capture
    if isinstance(marker, Mapping):
        return str(marker.get("capture_name") or marker.get("capture") or "")
    return ""


def _marker_text(marker: CaptureRecord | Mapping[str, Any] | str) -> str:
    if isinstance(marker, str):
        return marker
    node = marker.node if isinstance(marker, CaptureRecord) else marker.get("node", marker)
    if isinstance(node, Mapping):
        for key in ("text", "name", "label", "id", "module", "value"):
            value = node.get(key)
            if isinstance(value, str) and value:
                return value
    return ""


def _marker_path(marker: CaptureRecord | Mapping[str, Any] | str) -> str:
    node = marker.node if isinstance(marker, CaptureRecord) else marker
    if isinstance(node, Mapping):
        value = node.get("path")
        return str(value) if value else ""
    return ""


def _marker_span(marker: CaptureRecord | Mapping[str, Any] | str) -> tuple[int, int] | None:
    node = marker.node if isinstance(marker, CaptureRecord) else marker
    if not isinstance(node, Mapping):
        return None
    line_start = node.get("line_start")
    line_end = node.get("line_end", line_start)
    if isinstance(line_start, int) and isinstance(line_end, int):
        return line_start, line_end
    return None


def _marker_matches(text: str, marker: str) -> bool:
    normalized_marker = marker.lower()
    return text == normalized_marker or text.startswith(f"{normalized_marker}.") or normalized_marker in text


def _route_rule() -> RuntimeSurfaceRule:
    return RuntimeSurfaceRule("route", "Route", method_source="marker", path_source="argument", handler_source="owner")


def _endpoint_rule() -> RuntimeSurfaceRule:
    return RuntimeSurfaceRule("endpoint", "APIEndpoint", method_source="marker", path_source="argument", handler_source="owner")


def _component_rule() -> RuntimeSurfaceRule:
    return RuntimeSurfaceRule("component", "Component", handler_source="declaration")


BUILTIN_FRAMEWORK_PROFILES = (
    FrameworkProfile(
        name="fastapi",
        ecosystems=("pypi",),
        dependency_markers=("fastapi",),
        import_markers=("fastapi", "APIRouter", "FastAPI"),
        decorator_markers=("@app.", "@router.", "app.", "router."),
        call_markers=("FastAPI", "APIRouter"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="flask",
        ecosystems=("pypi",),
        dependency_markers=("flask",),
        import_markers=("flask", "Flask", "Blueprint"),
        decorator_markers=("@app.route", "@blueprint.route", "app.route", "blueprint.route"),
        call_markers=("Flask", "Blueprint"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="django",
        ecosystems=("pypi",),
        dependency_markers=("django",),
        import_markers=("django", "django.urls"),
        call_markers=("path", "re_path", "include"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="express",
        ecosystems=("npm",),
        dependency_markers=("express",),
        import_markers=("express",),
        call_markers=("express", "app.get", "app.post", "router.get", "router.post"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="next",
        ecosystems=("npm",),
        dependency_markers=("next",),
        import_markers=("next", "next/router", "next/navigation"),
        call_markers=("NextResponse", "generateStaticParams"),
        runtime_surfaces=(_route_rule(), _component_rule()),
    ),
    FrameworkProfile(
        name="gin",
        ecosystems=("go",),
        dependency_markers=("github.com/gin-gonic/gin",),
        import_markers=("github.com/gin-gonic/gin", "gin"),
        call_markers=("gin.Default", "gin.New", "gin.RouterGroup"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="axum",
        ecosystems=("cargo",),
        dependency_markers=("axum",),
        import_markers=("axum", "axum::routing"),
        call_markers=("Router::new", "axum::routing::get", "axum::routing::post"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
    FrameworkProfile(
        name="actix-web",
        ecosystems=("cargo",),
        dependency_markers=("actix-web",),
        import_markers=("actix_web", "actix-web"),
        decorator_markers=("#[get", "#[post", "#[route"),
        call_markers=("App::new", "actix_web::web::resource", "actix_web::web::scope"),
        runtime_surfaces=(_route_rule(), _endpoint_rule()),
    ),
)
