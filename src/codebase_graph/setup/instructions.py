from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

START_MARKER = "<!-- codebaseGraph:start -->"
END_MARKER = "<!-- codebaseGraph:end -->"


@dataclass(frozen=True, slots=True)
class InstructionResult:
    """Store the result of instruction operations."""
    action: str
    path: str | None

    def as_dict(self) -> dict[str, str | None]:
        """Return a JSON-serializable dictionary representation.

        Returns:
            A dictionary containing the computed payload.
        """
        return {"action": self.action, "path": self.path}


def upsert_instruction_block(
    repo_root: Path,
    *,
    target: str = "auto",
    server_name: str,
    config_path: Path,
    setup_command: str = "codebase-graph",
) -> InstructionResult:
    """Upsert instruction block.

    Args:
        repo_root: Repo root value.
        target: Target value.
        server_name: Server name value.
        config_path: The config path to read or write.
        setup_command: Setup command value.

    Returns:
        The computed result.
    """
    if target == "skip":
        return InstructionResult("skipped", None)
    path = _select_instruction_path(repo_root, target)
    existing = path.read_text(encoding="utf-8") if path.exists() else ""
    block = _instruction_block(server_name=server_name, config_path=config_path, setup_command=setup_command)
    next_text, action = _upsert_block(existing, block, created=not path.exists())
    if next_text == existing:
        return InstructionResult("unchanged", path.as_posix())
    path.write_text(next_text, encoding="utf-8")
    return InstructionResult(action, path.as_posix())


def instruction_target_path(repo_root: Path, *, target: str = "auto") -> Path | None:
    """Process instruction target path.

    Args:
        repo_root: Repo root value.
        target: Target value.

    Returns:
        The computed result.
    """
    if target == "skip":
        return None
    return _select_instruction_path(repo_root, target)


def remove_instruction_block(path: Path) -> bool:
    """Remove instruction block.

    Args:
        path: The path to read or write.

    Returns:
        Whether the check succeeds.
    """
    if not path.exists():
        return False
    existing = path.read_text(encoding="utf-8")
    start = existing.find(START_MARKER)
    end = existing.find(END_MARKER)
    if start == -1 or end == -1 or end < start:
        return False
    after_end = end + len(END_MARKER)
    next_text = (existing[:start].rstrip() + "\n\n" + existing[after_end:].lstrip()).strip() + "\n"
    path.write_text(next_text, encoding="utf-8")
    return True


def _select_instruction_path(repo_root: Path, target: str) -> Path:
    """Select instruction path.

    Args:
        repo_root: Repo root value.
        target: Target value.

    Returns:
        The computed result.
    """
    if target == "agents":
        return repo_root / "AGENTS.md"
    if target == "claude":
        return repo_root / "CLAUDE.md"
    if target != "auto":
        raise ValueError(f"Unsupported instruction target: {target}")
    agents = repo_root / "AGENTS.md"
    claude = repo_root / "CLAUDE.md"
    if agents.exists():
        return agents
    if claude.exists():
        return claude
    return agents


def _instruction_block(*, server_name: str, config_path: Path, setup_command: str) -> str:
    """Process instruction block.

    Args:
        server_name: Server name value.
        config_path: The config path to read or write.
        setup_command: Setup command value.

    Returns:
        The computed string.
    """
    return (
        f"{START_MARKER}\n"
        "## codebaseGraph workflow\n"
        "- Treat the repo-local `.codebaseGraph` graph as the project operating source of truth. It is prohibited to read the code source before you find the target files using the graph.\n"
        f"- Prefer the `{server_name}` MCP server tools over shell commands whenever they are exposed in the current agent session.\n"
        "- AI agents receive block output by default for `graph_search` and `graph_context`; request `output_format: \"json\"` or `include_structured_content: true` only for tests, APIs, or explicit structured-payload debugging.\n"
        "- Use MCP `graph_search` with `detail: \"slim\"` and `context_limit: 1` before answering repo-structure questions or performing coding tasks.\n"
        "- Use MCP `graph_context` with `profile: \"<profile>\"`, `detail: \"slim\"`, and `context_limit: 2` when relationships or nearby evidence matter; useful profiles include `definitions`, `dependencies`, `callgraph`, `docs`, `runtime`, and `change_impact`.\n"
        "- For architecture orientation, use MCP `graph_architecture_queries`, then execute selected read-only statements with MCP `graph_query`.\n"
        "- Use MCP `graph_schema` or `graph_query_helpers` before writing raw graph queries, and keep `graph_query` read-only.\n"
        f"- If MCP tools are unavailable, fall back to CLI: `{setup_command} graph-search <query> --repo-root . --no-refresh --detail slim --context-limit 1`, `{setup_command} graph-context <query> --repo-root . --profile <profile> --no-refresh --detail slim --context-limit 2`, `{setup_command} graph-architecture-queries`, `{setup_command} graph-query \"<statement>\" --repo-root .`, `{setup_command} graph-schema`, and `{setup_command} graph-query-helpers`.\n"
        f"- Refresh the graph with `{setup_command} setup --repo-root . --mcp-client none` when files change materially. Setup config: `{config_path.as_posix()}`.\n"
        f"{END_MARKER}\n"
    )


def _upsert_block(existing: str, block: str, *, created: bool) -> tuple[str, str]:
    """Upsert block.

    Args:
        existing: Existing value.
        block: Block value.
        created: Created value.

    Returns:
        A tuple containing the computed values.
    """
    if not existing.strip():
        return block, "created"
    start = existing.find(START_MARKER)
    end = existing.find(END_MARKER)
    if start != -1 and end != -1 and end > start:
        after_end = end + len(END_MARKER)
        return _join_sections(existing[:start], block, existing[after_end:]), "updated"
    separator = "" if existing.endswith("\n") else "\n"
    action = "created" if created else "updated"
    return existing.rstrip() + separator + "\n" + block, action


def _join_sections(prefix: str, block: str, suffix: str) -> str:
    """Join sections.

    Args:
        prefix: Prefix value.
        block: Block value.
        suffix: Suffix value.

    Returns:
        The computed string.
    """
    sections = [section.strip() for section in (prefix, block, suffix) if section.strip()]
    return "\n\n".join(sections) + "\n"
