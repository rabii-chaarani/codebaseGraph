from __future__ import annotations

import ast
import hashlib
import posixpath
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

CODE_EXTENSIONS = {".py"}
MAX_INDEXED_FILE_BYTES = 1_000_000
EXCLUDED_FILENAMES = {".DS_Store"}
EXCLUDED_PARTS = {
    ".git",
    ".hg",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".venv",
    ".codebase_graph",
    "__pycache__",
    "build",
    "dist",
    "htmlcov",
    "node_modules",
    "site-packages",
}

@dataclass(slots=True)
class CodeSymbol:
    id: str
    label: str
    kind: str
    path: str
    module_name: str
    qualified_name: str
    line_start: int | None = None
    line_end: int | None = None
    decorators: list[str] = field(default_factory=list)
    bases: list[str] = field(default_factory=list)
    summary: str = ""

@dataclass(slots=True)
class CodeFile:
    id: str
    path: str
    module_name: str
    language: str
    line_count: int
    summary: str = ""
    imports: list[str] = field(default_factory=list)
    calls: list[str] = field(default_factory=list)
    symbols: list[CodeSymbol] = field(default_factory=list)

@dataclass(slots=True)
class CodebaseMap:
    files: list[CodeFile]

    def as_dict(self) -> dict[str, Any]:
        return {"files": [_file_as_dict(file) for file in self.files]}

class CodebaseGraphBuilder:
    def __init__(self, root: str | Path) -> None:
        self.root = Path(root)

    def build(self) -> CodebaseMap:
        files = [_parse_python_file(path, self.root) for path in _iter_python_files(self.root)]
        return CodebaseMap(files=files)

def is_excluded_codebase_path_parts(parts: tuple[str, ...]) -> bool:
    return any(part in EXCLUDED_PARTS for part in parts)

def _iter_python_files(root: Path) -> list[Path]:
    if not root.exists():
        return []
    paths: list[Path] = []
    for path in root.rglob("*.py"):
        if not path.is_file() or path.name in EXCLUDED_FILENAMES:
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

def _parse_python_file(path: Path, root: Path) -> CodeFile:
    rel_path = path.relative_to(root).as_posix()
    text = path.read_text(encoding="utf-8", errors="replace")
    module_name = _module_name(rel_path)
    try:
        tree = ast.parse(text)
    except SyntaxError:
        return CodeFile(_id("file", rel_path), rel_path, module_name, "python", len(text.splitlines()), "Syntax error")

    imports: list[str] = []
    calls: list[str] = []
    symbols: list[CodeSymbol] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imports.extend(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            module = "." * node.level + (node.module or "")
            imports.append(module)
        elif isinstance(node, ast.Call):
            calls.append(_call_name(node.func))

    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            class_qn = f"{module_name}.{node.name}" if module_name else node.name
            symbols.append(
                CodeSymbol(
                    id=_id("symbol", class_qn),
                    label=node.name,
                    kind="python_class",
                    path=rel_path,
                    module_name=module_name,
                    qualified_name=class_qn,
                    line_start=getattr(node, "lineno", None),
                    line_end=getattr(node, "end_lineno", None),
                    decorators=[_call_name(item) for item in node.decorator_list],
                    bases=[_call_name(item) for item in node.bases],
                    summary=ast.get_docstring(node) or "",
                )
            )
            for child in node.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    method_qn = f"{class_qn}.{child.name}"
                    symbols.append(
                        CodeSymbol(
                            id=_id("symbol", method_qn),
                            label=child.name,
                            kind="python_method",
                            path=rel_path,
                            module_name=module_name,
                            qualified_name=method_qn,
                            line_start=getattr(child, "lineno", None),
                            line_end=getattr(child, "end_lineno", None),
                            decorators=[_call_name(item) for item in child.decorator_list],
                            summary=ast.get_docstring(child) or "",
                        )
                    )
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            function_qn = f"{module_name}.{node.name}" if module_name else node.name
            symbols.append(
                CodeSymbol(
                    id=_id("symbol", function_qn),
                    label=node.name,
                    kind="python_function",
                    path=rel_path,
                    module_name=module_name,
                    qualified_name=function_qn,
                    line_start=getattr(node, "lineno", None),
                    line_end=getattr(node, "end_lineno", None),
                    decorators=[_call_name(item) for item in node.decorator_list],
                    summary=ast.get_docstring(node) or "",
                )
            )

    return CodeFile(
        id=_id("file", rel_path),
        path=rel_path,
        module_name=module_name,
        language="python",
        line_count=len(text.splitlines()),
        summary=ast.get_docstring(tree) or "",
        imports=sorted(set(imports)),
        calls=sorted({call for call in calls if call}),
        symbols=symbols,
    )

def _module_name(rel_path: str) -> str:
    without_suffix = rel_path[:-3] if rel_path.endswith(".py") else rel_path
    parts = without_suffix.split("/")
    if parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(part for part in parts if part)

def _call_name(node: ast.AST) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = _call_name(node.value)
        return f"{base}.{node.attr}" if base else node.attr
    if isinstance(node, ast.Constant):
        return repr(node.value)
    return ""

def _id(prefix: str, value: str) -> str:
    return f"{prefix}:{hashlib.sha1(value.encode('utf-8')).hexdigest()[:20]}"

def _file_as_dict(file: CodeFile) -> dict[str, Any]:
    return {
        "id": file.id,
        "path": file.path,
        "module_name": file.module_name,
        "language": file.language,
        "line_count": file.line_count,
        "summary": file.summary,
        "imports": file.imports,
        "calls": file.calls,
        "symbols": [symbol.__dict__ for symbol in file.symbols],
    }

def relative_posix(path: Path, root: Path) -> str:
    return posixpath.join(*path.relative_to(root).parts)
