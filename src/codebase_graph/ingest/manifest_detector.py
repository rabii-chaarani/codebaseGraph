from __future__ import annotations

import configparser
import hashlib
import json
import re
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from codebase_graph.core import CodeGraph, GraphEdge, GraphNode

try:  # pragma: no cover - exercised on Python 3.10 only.
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore[no-redef]


@dataclass(frozen=True, slots=True)
class DependencyManifestSpec:
    """Manifest parser specification for one package or build ecosystem."""

    ecosystem: str
    path_patterns: tuple[str, ...]
    dependency_sections: tuple[str, ...] = ()
    lockfile_patterns: tuple[str, ...] = ()
    source_manifest: str = ""


@dataclass(frozen=True, slots=True)
class DependencyEvidence:
    """Dependency declaration extracted from a supported manifest."""

    ecosystem: str
    name: str
    version: str
    source_path: str
    source_manifest: str
    confidence: float = 1.0
    metadata: Mapping[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        """Serialize evidence for diagnostics or graph metadata."""
        return {
            "ecosystem": self.ecosystem,
            "name": self.name,
            "version": self.version,
            "source_path": self.source_path,
            "source_manifest": self.source_manifest,
            "confidence": self.confidence,
            "metadata": dict(self.metadata),
        }


DEFAULT_DEPENDENCY_MANIFEST_SPECS = (
    DependencyManifestSpec("pypi", ("pyproject.toml",), ("project.dependencies", "project.optional-dependencies")),
    DependencyManifestSpec("cargo", ("Cargo.toml",), ("dependencies", "dev-dependencies", "build-dependencies")),
    DependencyManifestSpec("go", ("go.mod",), ("require",)),
    DependencyManifestSpec("npm", ("package.json",), ("dependencies", "devDependencies", "peerDependencies")),
    DependencyManifestSpec("cmake", ("CMakeLists.txt",), ("find_package", "target_link_libraries")),
    DependencyManifestSpec("conan", ("conanfile.txt",), ("requires",)),
    DependencyManifestSpec("vcpkg", ("vcpkg.json",), ("dependencies",)),
    DependencyManifestSpec("fortran", ("fpm.toml",), ("dependencies", "dev-dependencies")),
)

EXCLUDED_PARTS = {
    ".git",
    ".venv",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "vendor",
}


def detect_dependency_manifest(
    source_root: str | Path,
    specs: Sequence[DependencyManifestSpec] = DEFAULT_DEPENDENCY_MANIFEST_SPECS,
) -> tuple[Path, ...]:
    """Identify supported dependency manifests below a source root."""
    root = Path(source_root)
    matches: dict[str, Path] = {}
    for spec in specs:
        for pattern in spec.path_patterns:
            iterator = root.glob(pattern) if "/" in pattern else root.rglob(pattern)
            for path in iterator:
                if path.is_file() and not _is_excluded(path, root):
                    matches[path.relative_to(root).as_posix()] = path
    return tuple(matches[key] for key in sorted(matches))


def parse_dependency_manifest(
    path: str | Path,
    *,
    source_root: str | Path | None = None,
) -> tuple[DependencyEvidence, ...]:
    """Parse one supported manifest into dependency evidence."""
    manifest_path = Path(path)
    root = Path(source_root) if source_root is not None else manifest_path.parent
    source_path = _relative_path(manifest_path, root)
    ecosystem = classify_dependency_ecosystem(manifest_path)
    if ecosystem == "pypi":
        return _parse_pyproject(manifest_path, source_path)
    if ecosystem == "cargo":
        return _parse_toml_dependency_tables(manifest_path, source_path, ecosystem, ("dependencies", "dev-dependencies"))
    if ecosystem == "go":
        return _parse_go_mod(manifest_path, source_path)
    if ecosystem == "npm":
        return _parse_package_json(manifest_path, source_path)
    if ecosystem == "cmake":
        return _parse_cmake(manifest_path, source_path)
    if ecosystem == "conan":
        return _parse_conanfile(manifest_path, source_path)
    if ecosystem == "vcpkg":
        return _parse_vcpkg(manifest_path, source_path)
    if ecosystem == "fortran":
        return _parse_toml_dependency_tables(manifest_path, source_path, ecosystem, ("dependencies", "dev-dependencies"))
    return ()


def classify_dependency_ecosystem(path: str | Path) -> str:
    """Normalize a manifest path into a dependency ecosystem key."""
    name = Path(path).name
    if name == "pyproject.toml":
        return "pypi"
    if name == "Cargo.toml":
        return "cargo"
    if name == "go.mod":
        return "go"
    if name == "package.json":
        return "npm"
    if name == "CMakeLists.txt":
        return "cmake"
    if name == "conanfile.txt":
        return "conan"
    if name == "vcpkg.json":
        return "vcpkg"
    if name == "fpm.toml":
        return "fortran"
    return "unknown"


def link_dependency_evidence(
    graph: CodeGraph,
    evidence: DependencyEvidence,
    *,
    owner_id: str | None = None,
) -> GraphNode:
    """Link dependency evidence to Dependency nodes and source-file evidence."""
    dependency = GraphNode(
        id=_stable_id("Dependency", f"{evidence.ecosystem}:{evidence.name}"),
        table="Dependency",
        label=evidence.name,
        kind="dependency",
        path=evidence.source_path,
        qualified_name=evidence.name,
        summary=f"{evidence.ecosystem} dependency {evidence.name}",
        metadata={
            "canonical_key": f"{evidence.ecosystem}:{evidence.name}",
            "ecosystem": evidence.ecosystem,
            "version": evidence.version,
            "source_manifest": evidence.source_manifest,
            **dict(evidence.metadata),
        },
    )
    added = graph.add_node(dependency)
    source = _dependency_owner(graph, evidence.source_path, owner_id)
    if source is not None and source.table in {"Repository", "SourceRoot", "File", "Module", "Dependency", "Component"}:
        graph.add_edge(
            GraphEdge(
                id=_stable_id("DependsOn", f"{source.id}->{added.id}"),
                type="DependsOn",
                source_id=source.id,
                target_id=added.id,
                kind="manifest_dependency",
                confidence=evidence.confidence,
                metadata=evidence.as_dict(),
            )
        )
    file_node = _file_node_for_path(graph, evidence.source_path)
    if file_node is not None:
        graph.add_edge(
            GraphEdge(
                id=_stable_id("EvidencedBy", f"{added.id}->{file_node.id}"),
                type="EvidencedBy",
                source_id=added.id,
                target_id=file_node.id,
                kind="manifest_evidence",
                confidence=evidence.confidence,
                metadata=evidence.as_dict(),
            )
        )
    return added


def enrich_dependency_context(
    source_root: str | Path,
    *,
    graph: CodeGraph | None = None,
) -> tuple[DependencyEvidence, ...]:
    """Find manifests, parse dependencies, and optionally attach evidence to a graph."""
    evidences: list[DependencyEvidence] = []
    for manifest_path in detect_dependency_manifest(source_root):
        for evidence in parse_dependency_manifest(manifest_path, source_root=source_root):
            evidences.append(evidence)
            if graph is not None:
                link_dependency_evidence(graph, evidence)
    return tuple(evidences)


def _parse_pyproject(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    payload = tomllib.loads(path.read_text(encoding="utf-8"))
    evidence: list[DependencyEvidence] = []
    for requirement in payload.get("project", {}).get("dependencies", ()):
        name, version = _split_requirement(str(requirement))
        evidence.append(_evidence("pypi", name, version, source_path, "pyproject.toml", "project.dependencies"))
    for group, requirements in payload.get("project", {}).get("optional-dependencies", {}).items():
        for requirement in requirements:
            name, version = _split_requirement(str(requirement))
            evidence.append(_evidence("pypi", name, version, source_path, "pyproject.toml", f"optional.{group}"))
    for requirement in payload.get("build-system", {}).get("requires", ()):
        name, version = _split_requirement(str(requirement))
        evidence.append(_evidence("pypi", name, version, source_path, "pyproject.toml", "build-system.requires"))
    return tuple(evidence)


def _parse_toml_dependency_tables(
    path: Path,
    source_path: str,
    ecosystem: str,
    table_names: Iterable[str],
) -> tuple[DependencyEvidence, ...]:
    payload = tomllib.loads(path.read_text(encoding="utf-8"))
    evidence: list[DependencyEvidence] = []
    for table_name in table_names:
        dependencies = payload.get(table_name, {})
        if not isinstance(dependencies, Mapping):
            continue
        for name, value in dependencies.items():
            version = _dependency_value_version(value)
            evidence.append(_evidence(ecosystem, str(name), version, source_path, path.name, table_name))
    return tuple(evidence)


def _parse_go_mod(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    evidence: list[DependencyEvidence] = []
    in_require_block = False
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.split("//", 1)[0].strip()
        if not stripped:
            continue
        if stripped == "require (":
            in_require_block = True
            continue
        if in_require_block and stripped == ")":
            in_require_block = False
            continue
        if stripped.startswith("require "):
            stripped = stripped.removeprefix("require ").strip()
        elif not in_require_block:
            continue
        parts = stripped.split()
        if len(parts) >= 2:
            evidence.append(_evidence("go", parts[0], parts[1], source_path, "go.mod", "require"))
    return tuple(evidence)


def _parse_package_json(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    evidence: list[DependencyEvidence] = []
    for section in ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies"):
        dependencies = payload.get(section, {})
        if not isinstance(dependencies, Mapping):
            continue
        for name, version in dependencies.items():
            evidence.append(_evidence("npm", str(name), str(version), source_path, "package.json", section))
    return tuple(evidence)


def _parse_cmake(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    text = path.read_text(encoding="utf-8")
    evidence: list[DependencyEvidence] = []
    for match in re.finditer(r"\bfind_package\s*\(\s*([A-Za-z0-9_.:+-]+)(?:\s+([^\)\s]+))?", text):
        evidence.append(_evidence("cmake", match.group(1), match.group(2) or "", source_path, "CMakeLists.txt", "find_package"))
    for match in re.finditer(r"\btarget_link_libraries\s*\(([^)]+)\)", text, flags=re.MULTILINE):
        for name in match.group(1).split()[1:]:
            if name and not name.startswith("$"):
                evidence.append(_evidence("cmake", name, "", source_path, "CMakeLists.txt", "target_link_libraries"))
    return tuple(evidence)


def _parse_conanfile(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    parser = configparser.ConfigParser(allow_no_value=True)
    parser.read(path, encoding="utf-8")
    evidence: list[DependencyEvidence] = []
    for requirement in parser["requires"] if parser.has_section("requires") else ():
        name, version = _split_conan_requirement(requirement)
        evidence.append(_evidence("conan", name, version, source_path, "conanfile.txt", "requires"))
    return tuple(evidence)


def _parse_vcpkg(path: Path, source_path: str) -> tuple[DependencyEvidence, ...]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    evidence: list[DependencyEvidence] = []
    dependencies = payload.get("dependencies", ())
    for item in dependencies:
        if isinstance(item, str):
            evidence.append(_evidence("vcpkg", item, "", source_path, "vcpkg.json", "dependencies"))
        elif isinstance(item, Mapping) and "name" in item:
            evidence.append(_evidence("vcpkg", str(item["name"]), "", source_path, "vcpkg.json", "dependencies"))
    return tuple(evidence)


def _evidence(
    ecosystem: str,
    name: str,
    version: str,
    source_path: str,
    source_manifest: str,
    section: str,
) -> DependencyEvidence:
    return DependencyEvidence(
        ecosystem=ecosystem,
        name=name,
        version=version,
        source_path=source_path,
        source_manifest=source_manifest,
        metadata={"section": section},
    )


def _dependency_value_version(value: Any) -> str:
    if isinstance(value, str):
        return value
    if isinstance(value, Mapping):
        version = value.get("version", "")
        return str(version) if version is not None else ""
    return ""


def _split_requirement(requirement: str) -> tuple[str, str]:
    match = re.match(r"^\s*([A-Za-z0-9_.-]+(?:\[[^]]+\])?)\s*(.*)$", requirement)
    if match is None:
        return requirement.strip(), ""
    name = match.group(1).split("[", 1)[0]
    return name, match.group(2).strip()


def _split_conan_requirement(requirement: str) -> tuple[str, str]:
    if "/" not in requirement:
        return requirement.strip(), ""
    name, version = requirement.split("/", 1)
    return name.strip(), version.strip()


def _dependency_owner(graph: CodeGraph, source_path: str, owner_id: str | None) -> GraphNode | None:
    if owner_id:
        return graph.nodes.get(owner_id)
    return _file_node_for_path(graph, source_path)


def _file_node_for_path(graph: CodeGraph, source_path: str) -> GraphNode | None:
    for node in graph.nodes.values():
        if node.table == "File" and node.path == source_path:
            return node
    return None


def _relative_path(path: Path, root: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.name


def _is_excluded(path: Path, root: Path) -> bool:
    try:
        parts = path.relative_to(root).parts
    except ValueError:
        parts = path.parts
    return any(part in EXCLUDED_PARTS or part.endswith(".egg-info") for part in parts)


def _stable_id(table: str, stable_key: str) -> str:
    digest = hashlib.sha1(f"{table}:{stable_key}".encode("utf-8")).hexdigest()[:20]
    return f"{table}:{digest}"
