from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - py310 fallback
    import tomli as tomllib  # type: ignore[no-redef]

from .code_map import CodebaseGraphBuilder, EXCLUDED_FILENAMES, MAX_INDEXED_FILE_BYTES, is_excluded_codebase_path_parts
from .document_layers import LogicalChunker
from .markdown import parse_markdown, plain_text
from .ontology import ONTOLOGY_NAME
from .verification import summarize_verification_run

@dataclass(slots=True)
class GraphExport:
    nodes: list[dict[str, Any]] = field(default_factory=list)
    edges: list[dict[str, Any]] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        return {"ontology": ONTOLOGY_NAME, "metadata": self.metadata, "nodes": self.nodes, "edges": self.edges}

    def summary(self) -> dict[str, Any]:
        node_counts: dict[str, int] = {}
        edge_counts: dict[str, int] = {}
        for node in self.nodes:
            node_counts[node.get("table", "Unknown")] = node_counts.get(node.get("table", "Unknown"), 0) + 1
        for edge in self.edges:
            edge_counts[edge.get("type", "Unknown")] = edge_counts.get(edge.get("type", "Unknown"), 0) + 1
        return {
            "ontology": ONTOLOGY_NAME,
            "node_count": len(self.nodes),
            "edge_count": len(self.edges),
            "node_counts": node_counts,
            "edge_counts": edge_counts,
        }

class ProductionGraphBuilder:
    def __init__(self, repo_root: str | Path = ".") -> None:
        self.repo_root = Path(repo_root)
        self.nodes: dict[str, dict[str, Any]] = {}
        self.edges: dict[str, dict[str, Any]] = {}

    def build_export(self) -> GraphExport:
        project_name = self._project_name()
        project_id = _id("project", project_name)
        repository_id = _id("repository", str(self.repo_root.resolve()))
        self._node("Project", project_id, project_name, "project", path=".")
        self._node("Repository", repository_id, self.repo_root.name, "repository", path=str(self.repo_root))
        self._edge("Contains", project_id, repository_id, "project_repository")
        self._add_codebase(repository_id)
        self._add_documentation(repository_id)
        self._add_dependencies(repository_id)
        self._add_entry_points(repository_id)
        self._add_verification_sources(repository_id)
        return GraphExport(
            nodes=sorted(self.nodes.values(), key=lambda item: (item.get("table", ""), item.get("id", ""))),
            edges=sorted(self.edges.values(), key=lambda item: item.get("id", "")),
            metadata={"project_name": project_name, "source_root": str(self.repo_root)},
        )

    def _add_codebase(self, repository_id: str) -> None:
        code_map = CodebaseGraphBuilder(self.repo_root).build()
        for file in code_map.files:
            file_node_id = file.id
            self._node(
                "File",
                file_node_id,
                Path(file.path).name,
                "python_file",
                path=file.path,
                module_name=file.module_name,
                summary=file.summary,
                metadata={"line_count": file.line_count, "language": file.language},
            )
            self._edge("Contains", repository_id, file_node_id, "repository_file")
            module_id = _id("module", file.module_name or file.path)
            self._node(
                "PythonModule",
                module_id,
                file.module_name or file.path,
                "python_module",
                path=file.path,
                module_name=file.module_name,
                qualified_name=file.module_name,
                summary=file.summary,
            )
            self._edge("Defines", file_node_id, module_id, "file_module")
            for imported in file.imports:
                import_id = _id("import", f"{file.path}:{imported}")
                self._node("Import", import_id, imported, "python_import", path=file.path, qualified_name=imported)
                self._edge("Imports", module_id, import_id, "module_import")
            for call in file.calls:
                call_id = _id("call", f"{file.path}:{call}")
                self._node("Call", call_id, call, "python_call", path=file.path, qualified_name=call)
                self._edge("Calls", module_id, call_id, "module_call")
            for symbol in file.symbols:
                table = {
                    "python_class": "PythonClass",
                    "python_function": "PythonFunction",
                    "python_method": "PythonMethod",
                }.get(symbol.kind, "PythonFunction")
                self._node(
                    table,
                    symbol.id,
                    symbol.label,
                    symbol.kind,
                    path=symbol.path,
                    module_name=symbol.module_name,
                    qualified_name=symbol.qualified_name,
                    line_start=symbol.line_start,
                    line_end=symbol.line_end,
                    summary=symbol.summary,
                    metadata={"decorators": symbol.decorators, "bases": symbol.bases},
                )
                self._edge("Defines", module_id, symbol.id, "module_symbol")

    def _add_documentation(self, repository_id: str) -> None:
        chunker = LogicalChunker(max_chars=1200)
        for path in _iter_documentation_files(self.repo_root):
            rel_path = path.relative_to(self.repo_root).as_posix()
            content = path.read_text(encoding="utf-8", errors="replace")
            _, body = parse_markdown(content) if path.suffix.lower() == ".md" else ({}, content)
            summary = plain_text(body)[:500]
            doc_id = _id("doc", rel_path)
            self._node(
                "DocumentationSource",
                doc_id,
                path.name,
                "documentation_source",
                path=rel_path,
                summary=summary,
                metadata={"chunks": [_chunk_as_dict(chunk) for chunk in chunker.chunk(body)[:5]]},
            )
            self._edge("Describes", repository_id, doc_id, "repository_documentation")

    def _add_dependencies(self, repository_id: str) -> None:
        pyproject = self.repo_root / "pyproject.toml"
        if not pyproject.exists():
            return
        payload = tomllib.loads(pyproject.read_text(encoding="utf-8"))
        project = payload.get("project", {}) if isinstance(payload, dict) else {}
        dependencies = project.get("dependencies", []) if isinstance(project, dict) else []
        for dependency in dependencies:
            name = str(dependency).split(";", 1)[0].strip()
            dep_id = _id("dependency", name)
            self._node("Dependency", dep_id, name, "python_dependency", path="pyproject.toml", summary=str(dependency))
            self._edge("DependsOn", repository_id, dep_id, "declared_dependency")

    def _add_entry_points(self, repository_id: str) -> None:
        pyproject = self.repo_root / "pyproject.toml"
        if not pyproject.exists():
            return
        payload = tomllib.loads(pyproject.read_text(encoding="utf-8"))
        scripts = payload.get("project", {}).get("scripts", {}) if isinstance(payload, dict) else {}
        if not isinstance(scripts, dict):
            return
        for name, target in sorted(scripts.items()):
            entry_id = _id("entry", f"script:{name}")
            self._node("EntryPoint", entry_id, name, "console_script", path="pyproject.toml", qualified_name=str(target))
            self._edge("Produces", repository_id, entry_id, "repository_entry_point")

    def _add_verification_sources(self, repository_id: str) -> None:
        for directory in (self.repo_root / ".codebase_graph" / "verification_runs", self.repo_root / "verification"):
            if not directory.exists():
                continue
            for path in sorted(directory.glob("*.json")):
                try:
                    payload = json.loads(path.read_text(encoding="utf-8"))
                except json.JSONDecodeError:
                    continue
                command = str(payload.get("command", ""))
                output = str(payload.get("output", ""))
                exit_code = payload.get("exit_code")
                summary = summarize_verification_run(command, output, exit_code if isinstance(exit_code, int) else None)
                verification_id = _id("verification", path.relative_to(self.repo_root).as_posix())
                self._node(
                    "Verification",
                    verification_id,
                    summary["tool"],
                    "verification_run",
                    path=path.relative_to(self.repo_root).as_posix(),
                    summary=summary["summary"],
                    metadata=summary,
                )
                self._edge("Produces", repository_id, verification_id, "repository_verification")

    def _project_name(self) -> str:
        pyproject = self.repo_root / "pyproject.toml"
        if pyproject.exists():
            try:
                payload = tomllib.loads(pyproject.read_text(encoding="utf-8"))
                name = payload.get("project", {}).get("name")
                if name:
                    return str(name)
            except Exception:
                pass
        return self.repo_root.name

    def _node(self, table: str, node_id: str, label: str, kind: str, **fields: Any) -> None:
        existing = self.nodes.get(node_id, {})
        node = {
            "id": node_id,
            "table": table,
            "label": label,
            "kind": kind,
            "path": fields.pop("path", ""),
            "qualified_name": fields.pop("qualified_name", ""),
            "module_name": fields.pop("module_name", ""),
            "line_start": fields.pop("line_start", None),
            "line_end": fields.pop("line_end", None),
            "summary": fields.pop("summary", ""),
            "metadata": fields.pop("metadata", {}),
        }
        node.update(fields)
        existing.update({key: value for key, value in node.items() if value not in (None, "", {})})
        self.nodes[node_id] = existing or node

    def _edge(self, edge_type: str, source_id: str, target_id: str, kind: str, **fields: Any) -> None:
        edge_id = _id("edge", f"{edge_type}:{source_id}:{target_id}:{kind}")
        self.edges[edge_id] = {
            "id": edge_id,
            "type": edge_type,
            "kind": kind,
            "source_id": source_id,
            "target_id": target_id,
            "metadata": fields,
        }


def _chunk_as_dict(chunk: Any) -> dict[str, Any]:
    return {"id": chunk.id, "heading": chunk.heading, "text": chunk.text, "ordinal": chunk.ordinal}

def _iter_documentation_files(root: Path) -> list[Path]:
    suffixes = {".md", ".txt", ".rst"}
    paths: list[Path] = []
    if not root.exists():
        return paths
    for path in root.rglob("*"):
        if not path.is_file() or path.suffix.lower() not in suffixes or path.name in EXCLUDED_FILENAMES:
            continue
        try:
            rel_parts = path.relative_to(root).parts
        except ValueError:
            continue
        if is_excluded_codebase_path_parts(rel_parts):
            continue
        try:
            if path.stat().st_size > MAX_INDEXED_FILE_BYTES:
                continue
        except OSError:
            continue
        paths.append(path)
    return sorted(paths)

def _id(prefix: str, value: str) -> str:
    return f"{prefix}:{hashlib.sha1(value.encode('utf-8')).hexdigest()[:20]}"
