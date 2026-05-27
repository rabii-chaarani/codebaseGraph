from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from codebase_graph.diagnostics import log_event
from codebase_graph.ingest import GraphMaterializer

from .instructions import InstructionResult, upsert_instruction_block
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
        paths.state_dir.mkdir(parents=True, exist_ok=True)
        mcp_entry = server_entry(paths.config_path)
        config_payload = build_setup_config(paths, mcp_command=[mcp_entry["command"], *mcp_entry["args"]])
        config_action = write_setup_config(paths.config_path, config_payload)
        instructions = upsert_instruction_block(
            paths.repo_root,
            target=options.instructions_target,
            server_name=MCP_SERVER_NAME,
            config_path=paths.config_path,
            setup_command=mcp_entry["command"],
        )
        materializer = GraphMaterializer(
            paths.repo_root,
            db_path=paths.db_path,
            manifest_path=paths.manifest_path,
            include_fts=True,
            repository_label=paths.repo_name,
        )
        try:
            materialization = materializer.materialize(mode=options.mode)  # type: ignore[arg-type]
        finally:
            materializer.close()
        mcp_result = configure_mcp_client(
            client=options.mcp_client,
            config_path=options.mcp_config_path,
            setup_config_path=paths.config_path,
            dry_run=options.dry_run,
            skip=options.skip_mcp_config,
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
    return {
        "mode": getattr(result, "mode"),
        "scanned": getattr(result, "scanned"),
        "rebuilt": getattr(result, "rebuilt"),
        "skipped": getattr(result, "skipped"),
        "deleted": getattr(result, "deleted"),
        "diagnostics": list(getattr(result, "diagnostics")),
        "manifest_path": getattr(result, "manifest_path"),
        "rebuilt_paths": list(getattr(result, "rebuilt_paths")),
        "skipped_paths": list(getattr(result, "skipped_paths")),
        "deleted_paths": list(getattr(result, "deleted_paths")),
        "graph_summary": dict(getattr(result, "graph_summary")),
    }
