from __future__ import annotations

import json
import shutil
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from codebase_graph.ingest.languages import LanguageProfile
else:
    LanguageProfile = Any

try:  # pragma: no cover - exercised on Python 3.10 only.
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore[no-redef]


@dataclass(frozen=True, slots=True)
class BuildTarget:
    """Compilation or package target that owns a group of source files."""

    name: str
    language: str
    source_paths: tuple[str, ...] = ()
    compiler_args: tuple[str, ...] = ()
    dependencies: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize target data for graph metadata and diagnostics."""
        return {
            "name": self.name,
            "language": self.language,
            "source_paths": list(self.source_paths),
            "compiler_args": list(self.compiler_args),
            "dependencies": list(self.dependencies),
        }


@dataclass(frozen=True, slots=True)
class ToolchainCapability:
    """Detected semantic tool availability for one language provider."""

    provider: str
    executable: str
    version: str = ""
    available: bool = False
    reason: str = ""

    def as_dict(self) -> dict[str, Any]:
        """Serialize provider capability data."""
        return {
            "provider": self.provider,
            "executable": self.executable,
            "version": self.version,
            "available": self.available,
            "reason": self.reason,
        }


@dataclass(frozen=True, slots=True)
class BuildContext:
    """Project-level build metadata snapshot used to constrain semantic resolution."""

    id: str
    ecosystem: str
    root_path: str
    manifest_path: str
    targets: tuple[BuildTarget, ...] = ()
    diagnostics: tuple[str, ...] = ()
    toolchains: tuple[ToolchainCapability, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize build context data for graph metadata."""
        return {
            "id": self.id,
            "ecosystem": self.ecosystem,
            "root_path": self.root_path,
            "manifest_path": self.manifest_path,
            "targets": [target.as_dict() for target in self.targets],
            "diagnostics": list(self.diagnostics),
            "toolchains": [toolchain.as_dict() for toolchain in self.toolchains],
        }


def collect_project_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
    profiles: Sequence[LanguageProfile] | None = None,
) -> BuildContext:
    """Coordinate ecosystem detection, manifest parsing, target mapping, and diagnostics."""
    return detect_build_context(source_root, source_paths=source_paths, profiles=profiles)


def detect_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
    profiles: Sequence[LanguageProfile] | None = None,
) -> BuildContext:
    """Detect the dominant build ecosystem without failing on missing metadata."""
    root = Path(source_root)
    profile_list = tuple(profiles) if profiles is not None else _load_language_profiles(root)
    paths = tuple(sorted(source_paths)) if source_paths is not None else _discover_source_paths(root, profile_list)
    diagnostics: list[str] = []
    targets: list[BuildTarget] = []
    manifest_paths: list[str] = []

    for parser in (
        parse_rust_build_context,
        parse_go_build_context,
        parse_c_family_build_context,
        parse_fortran_build_context,
    ):
        parsed = parser(root, source_paths=paths)
        targets.extend(parsed)
        for target in parsed:
            manifest = _target_manifest(target)
            if manifest:
                manifest_paths.append(manifest)

    covered = {path for target in targets for path in target.source_paths}
    for path in paths:
        if path not in covered:
            profile = _resolve_language_profile(path, profile_list)
            if profile is None:
                continue
            diagnostics.append(f"No build metadata matched {path}; using language-profile fallback.")
            targets.append(
                BuildTarget(
                    name=f"{profile.language}:fallback",
                    language=profile.language,
                    source_paths=(path,),
                )
            )

    if not targets:
        diagnostics.append("No supported build metadata or source targets found.")

    ecosystem = _dominant_ecosystem(targets)
    manifest_path = sorted(set(manifest_paths))[0] if manifest_paths else ""
    return BuildContext(
        id=f"{root.resolve().as_posix()}:{ecosystem}",
        ecosystem=ecosystem,
        root_path=root.as_posix(),
        manifest_path=manifest_path,
        targets=tuple(sorted(targets, key=lambda item: (item.language, item.name, item.source_paths))),
        diagnostics=tuple(diagnostics),
        toolchains=_detect_toolchains(),
    )


def parse_rust_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
) -> tuple[BuildTarget, ...]:
    """Read Cargo metadata and map Rust crates, modules, and targets."""
    root = Path(source_root)
    manifest = root / "Cargo.toml"
    paths = tuple(path for path in _paths_for_language(root, "rust", source_paths) if path.endswith(".rs"))
    if not manifest.exists() and not paths:
        return ()
    dependencies: tuple[str, ...] = ()
    name = "cargo"
    if manifest.exists():
        payload = tomllib.loads(manifest.read_text(encoding="utf-8"))
        package = payload.get("package", {})
        if isinstance(package, Mapping):
            name = str(package.get("name") or name)
        dependencies = tuple(
            sorted(
                str(dep)
                for section in ("dependencies", "dev-dependencies", "build-dependencies")
                for dep in _dependency_names(payload.get(section, {}))
            )
        )
    return (
        BuildTarget(
            name=name,
            language="rust",
            source_paths=paths,
            dependencies=dependencies,
            compiler_args=("manifest=Cargo.toml",) if manifest.exists() else (),
        ),
    )


def parse_go_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
) -> tuple[BuildTarget, ...]:
    """Read go.mod and package layout metadata."""
    root = Path(source_root)
    manifest = root / "go.mod"
    paths = tuple(path for path in _paths_for_language(root, "go", source_paths) if path.endswith(".go"))
    if not manifest.exists() and not paths:
        return ()
    module = "go"
    dependencies: list[str] = []
    if manifest.exists():
        in_require_block = False
        for line in manifest.read_text(encoding="utf-8").splitlines():
            stripped = line.split("//", 1)[0].strip()
            if stripped.startswith("module "):
                module = stripped.split(maxsplit=1)[1]
            elif stripped == "require (":
                in_require_block = True
            elif in_require_block and stripped == ")":
                in_require_block = False
            elif stripped.startswith("require "):
                dependencies.append(stripped.split()[1])
            elif in_require_block and stripped:
                dependencies.append(stripped.split()[0])
    return (
        BuildTarget(
            name=module,
            language="go",
            source_paths=paths,
            dependencies=tuple(sorted(set(dependencies))),
            compiler_args=(f"module={module}",) if manifest.exists() else (),
        ),
    )


def parse_c_family_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
) -> tuple[BuildTarget, ...]:
    """Read compile_commands.json and CMake hints for C and C++ source files."""
    root = Path(source_root)
    compile_commands = root / "compile_commands.json"
    c_paths = set(_paths_for_language(root, "c", source_paths))
    cpp_paths = set(_paths_for_language(root, "cpp", source_paths))
    targets: list[BuildTarget] = []
    if compile_commands.exists():
        payload = json.loads(compile_commands.read_text(encoding="utf-8"))
        by_language: dict[str, list[tuple[str, tuple[str, ...]]]] = {"c": [], "cpp": []}
        if isinstance(payload, list):
            for item in payload:
                if not isinstance(item, Mapping):
                    continue
                file_path = _relative_to_root(Path(str(item.get("file") or "")), root)
                if not file_path:
                    continue
                language = "cpp" if _is_cpp_path(file_path) else "c"
                args = tuple(str(arg) for arg in item.get("arguments", ()) if isinstance(arg, str))
                command = str(item.get("command") or "")
                by_language.setdefault(language, []).append((file_path, args or tuple(command.split())))
        for language, entries in by_language.items():
            if entries:
                targets.append(
                    BuildTarget(
                        name=f"{language}:compile_commands",
                        language=language,
                        source_paths=tuple(sorted(path for path, _args in entries)),
                        compiler_args=tuple(sorted({arg for _path, args in entries for arg in args})),
                    )
                )
    cmake = root / "CMakeLists.txt"
    if cmake.exists():
        if c_paths:
            targets.append(BuildTarget("c:cmake", "c", tuple(sorted(c_paths)), compiler_args=("manifest=CMakeLists.txt",)))
        if cpp_paths:
            targets.append(BuildTarget("cpp:cmake", "cpp", tuple(sorted(cpp_paths)), compiler_args=("manifest=CMakeLists.txt",)))
    if not targets:
        if c_paths:
            targets.append(BuildTarget("c:fallback", "c", tuple(sorted(c_paths))))
        if cpp_paths:
            targets.append(BuildTarget("cpp:fallback", "cpp", tuple(sorted(cpp_paths))))
    return tuple(targets)


def parse_fortran_build_context(
    source_root: str | Path,
    *,
    source_paths: Iterable[str] | None = None,
) -> tuple[BuildTarget, ...]:
    """Read Fortran project hints and map source units."""
    root = Path(source_root)
    manifest = root / "fpm.toml"
    paths = tuple(_paths_for_language(root, "fortran", source_paths))
    if not manifest.exists() and not paths:
        return ()
    dependencies: tuple[str, ...] = ()
    name = "fortran"
    if manifest.exists():
        payload = tomllib.loads(manifest.read_text(encoding="utf-8"))
        project = payload.get("project", {})
        if isinstance(project, Mapping):
            name = str(project.get("name") or name)
        dependencies = tuple(
            sorted(
                str(dep)
                for section in ("dependencies", "dev-dependencies")
                for dep in _dependency_names(payload.get(section, {}))
            )
        )
    return (
        BuildTarget(
            name=name,
            language="fortran",
            source_paths=paths,
            dependencies=dependencies,
            compiler_args=("manifest=fpm.toml",) if manifest.exists() else (),
        ),
    )


def map_source_to_build_target(context: BuildContext, source_path: str | Path) -> BuildTarget | None:
    """Map a source path to the most specific known build target."""
    path = Path(source_path).as_posix()
    matches = [target for target in context.targets if path in target.source_paths]
    if not matches:
        return None
    return sorted(matches, key=lambda item: (len(item.source_paths), item.language, item.name))[0]


def _detect_toolchains() -> tuple[ToolchainCapability, ...]:
    providers = {
        "rust_analyzer": "rust-analyzer",
        "gopls": "gopls",
        "clangd": "clangd",
        "fortls": "fortls",
    }
    capabilities: list[ToolchainCapability] = []
    for provider, executable in providers.items():
        path = shutil.which(executable)
        capabilities.append(
            ToolchainCapability(
                provider=provider,
                executable=path or executable,
                available=path is not None,
                reason="" if path is not None else "executable not found",
            )
        )
    return tuple(capabilities)


def _discover_source_paths(root: Path, profiles: Sequence[LanguageProfile]) -> tuple[str, ...]:
    suffixes = {suffix for profile in profiles for suffix in profile.suffixes}
    paths: list[str] = []
    for path in root.rglob("*"):
        if path.is_file() and path.suffix in suffixes and not _is_excluded(path, root):
            paths.append(path.relative_to(root).as_posix())
    return tuple(sorted(paths))


def _paths_for_language(root: Path, language: str, source_paths: Iterable[str] | None) -> tuple[str, ...]:
    profiles = _load_language_profiles(root)
    profile = _resolve_language_profile(language, profiles)
    suffixes = profile.suffixes if profile is not None else ()
    candidates = tuple(source_paths) if source_paths is not None else _discover_source_paths(root, profiles)
    return tuple(sorted(path for path in candidates if Path(path).suffix in suffixes))


def _dependency_names(value: Any) -> tuple[str, ...]:
    if isinstance(value, Mapping):
        return tuple(str(key) for key in value)
    return ()


def _load_language_profiles(root: Path) -> tuple[Any, ...]:
    from codebase_graph.ingest.languages import load_language_profiles

    return load_language_profiles(root)


def _resolve_language_profile(path_or_language: str | Path, profiles: Sequence[Any]) -> Any | None:
    from codebase_graph.ingest.languages import resolve_language_profile

    return resolve_language_profile(path_or_language, profiles)


def _dominant_ecosystem(targets: Sequence[BuildTarget]) -> str:
    ecosystems = sorted({target.language for target in targets})
    if not ecosystems:
        return "unknown"
    return ecosystems[0] if len(ecosystems) == 1 else "mixed"


def _target_manifest(target: BuildTarget) -> str:
    for arg in target.compiler_args:
        if arg.startswith("manifest="):
            return arg.split("=", 1)[1]
    if target.name.endswith(":compile_commands"):
        return "compile_commands.json"
    return ""


def _relative_to_root(path: Path, root: Path) -> str:
    if not path.as_posix():
        return ""
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def _is_cpp_path(path: str) -> bool:
    return Path(path).suffix.lower() in {".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".h++", ".c++"}


def _is_excluded(path: Path, root: Path) -> bool:
    parts = path.relative_to(root).parts
    return any(part in {".git", ".venv", "__pycache__", "build", "dist", "node_modules", "vendor"} for part in parts)
