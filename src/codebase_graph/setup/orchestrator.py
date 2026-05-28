from __future__ import annotations

import shutil
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from codebase_graph.diagnostics import log_event
from codebase_graph.ingest import GraphMaterializer

from .instructions import InstructionResult, instruction_target_path, upsert_instruction_block
from .mcp_config import McpConfigResult, configure_mcp_client, server_entry
from .preflight import validate_ladybug_runtime
from .state import MCP_SERVER_NAME, SetupPaths, build_setup_config, derive_setup_paths, write_setup_config


class SetupError(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class SetupOptions:
    repo_root: str | Path = "."
    mcp_client: str = "codex"
    mcp_config_path: str | Path | None = None
    skip_mcp_config: bool = False
    dry_run: bool = False
    instructions_target: str = "auto"
    mode: str = "changed"


@dataclass(frozen=True, slots=True)
class SetupResult:
    paths: SetupPaths
    config_action: str
    materialization: Any
    mcp_config: McpConfigResult
    instructions: InstructionResult
    legacy_state_detected: bool

    def as_dict(self) -> dict[str, Any]:
        return {
            **self.paths.as_dict(),
            "config_action": self.config_action,
            "legacy_state_detected": self.legacy_state_detected,
            "mcp_config": self.mcp_config.as_dict(),
            "instructions": self.instructions.as_dict(),
            "materialization": _materialization_payload(self.materialization),
        }


def run_setup(options: SetupOptions) -> SetupResult:
    try:
        log_event(
            "setup.start",
            level="INFO",
            repo_root=str(options.repo_root),
            mcp_client=options.mcp_client,
            dry_run=options.dry_run,
        )
        paths = derive_setup_paths(options.repo_root)
        validate_ladybug_runtime()
        mcp_entry = server_entry(paths.config_path)
        config_payload = build_setup_config(paths, mcp_command=[mcp_entry["command"], *mcp_entry["args"]])
        if options.dry_run:
            materialization = _dry_run_materialization(paths)
            config_action = "dry_run" if _config_would_change(paths.config_path, config_payload) else "unchanged"
            target_path = instruction_target_path(paths.repo_root, target=options.instructions_target)
            instructions = InstructionResult("dry_run" if target_path is not None else "skipped", _path_text(target_path))
        else:
            target_path = instruction_target_path(paths.repo_root, target=options.instructions_target)
            previous_config = _snapshot_file(paths.config_path)
            previous_instructions = _snapshot_file(target_path)
            state_dir_existed = paths.state_dir.exists()
            materializer = GraphMaterializer(
                paths.repo_root,
                db_path=paths.db_path,
                manifest_path=paths.manifest_path,
                include_fts=True,
                repository_label=paths.repo_name,
            )
            try:
                config_action = write_setup_config(paths.config_path, config_payload)
                instructions = upsert_instruction_block(
                    paths.repo_root,
                    target=options.instructions_target,
                    server_name=MCP_SERVER_NAME,
                    config_path=paths.config_path,
                    setup_command=mcp_entry["command"],
                )
                materialization = materializer.materialize(mode=options.mode)  # type: ignore[arg-type]
                mcp_result = configure_mcp_client(
                    client=options.mcp_client,
                    config_path=options.mcp_config_path,
                    setup_config_path=paths.config_path,
                    dry_run=False,
                    skip=options.skip_mcp_config,
                )
            except Exception:
                _restore_file(paths.config_path, previous_config)
                _restore_file(target_path, previous_instructions)
                if not state_dir_existed:
                    shutil.rmtree(paths.state_dir, ignore_errors=True)
                raise
            finally:
                materializer.close()
        if options.dry_run and not options.skip_mcp_config:
            mcp_result = configure_mcp_client(
                client=options.mcp_client,
                config_path=options.mcp_config_path,
                setup_config_path=paths.config_path,
                dry_run=True,
                skip=False,
            )
        elif options.dry_run:
            mcp_result = configure_mcp_client(
                client=options.mcp_client,
                config_path=options.mcp_config_path,
                setup_config_path=paths.config_path,
                dry_run=True,
                skip=True,
            )
    except Exception as exc:
        log_event(
            "setup.failed",
            level="ERROR",
            repo_root=str(options.repo_root),
            error_type=exc.__class__.__name__,
            message=str(exc),
        )
        if isinstance(exc, SetupError):
            raise
        raise SetupError(str(exc)) from exc
    log_event(
        "setup.completed",
        level="INFO",
        repo_root=paths.repo_root.as_posix(),
        config_action=config_action,
        rebuilt=getattr(materialization, "rebuilt"),
        deleted=getattr(materialization, "deleted"),
        mcp_action=mcp_result.action,
    )
    return SetupResult(
        paths=paths,
        config_action=config_action,
        materialization=materialization,
        mcp_config=mcp_result,
        instructions=instructions,
        legacy_state_detected=(paths.repo_root / ".codebase_graph").exists(),
    )


def _materialization_payload(result: Any) -> dict[str, Any]:
    as_dict = getattr(result, "as_dict", None)
    if callable(as_dict):
        return as_dict()
    raise TypeError(f"Unsupported materialization result: {type(result).__name__}")


def _dry_run_materialization(paths: SetupPaths) -> Any:
    materializer = GraphMaterializer(
        paths.repo_root,
        db_path=paths.db_path,
        manifest_path=paths.manifest_path,
        include_fts=True,
        repository_label=paths.repo_name,
    )
    try:
        snapshots, diagnostics = materializer._scan_source_files()
    finally:
        materializer.close()
    skipped_paths = tuple(sorted(path for path, snapshot in snapshots.items() if snapshot.language is None))
    return _DryRunMaterialization(
        scanned=len(snapshots),
        skipped=len(skipped_paths),
        diagnostics=tuple(diagnostics),
        manifest_path=paths.manifest_path.as_posix(),
        skipped_paths=skipped_paths,
    )


@dataclass(frozen=True, slots=True)
class _DryRunMaterialization:
    scanned: int
    skipped: int
    diagnostics: tuple[str, ...]
    manifest_path: str
    skipped_paths: tuple[str, ...]
    mode: str = "dry_run"
    rebuilt: int = 0
    deleted: int = 0
    rebuilt_paths: tuple[str, ...] = ()
    deleted_paths: tuple[str, ...] = ()
    graph_summary: dict[str, Any] = field(default_factory=dict)

    def as_dict(self) -> dict[str, Any]:
        return {
            "mode": self.mode,
            "scanned": self.scanned,
            "rebuilt": self.rebuilt,
            "skipped": self.skipped,
            "deleted": self.deleted,
            "diagnostics": list(self.diagnostics),
            "manifest_path": self.manifest_path,
            "rebuilt_paths": list(self.rebuilt_paths),
            "skipped_paths": list(self.skipped_paths),
            "deleted_paths": list(self.deleted_paths),
            "graph_summary": dict(self.graph_summary),
        }


def _config_would_change(path: Path, payload: dict[str, Any]) -> bool:
    if not path.exists():
        return True
    try:
        import json

        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle) != payload
    except Exception:
        return True


def _path_text(path: Path | None) -> str | None:
    return path.as_posix() if path is not None else None


def _snapshot_file(path: Path | None) -> str | None:
    if path is None or not path.exists():
        return None
    return path.read_text(encoding="utf-8")


def _restore_file(path: Path | None, previous: str | None) -> None:
    if path is None:
        return
    if previous is None:
        try:
            path.unlink()
        except FileNotFoundError:
            return
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(previous, encoding="utf-8")
