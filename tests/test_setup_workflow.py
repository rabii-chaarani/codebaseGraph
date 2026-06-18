from __future__ import annotations

import json
import re
from pathlib import Path

try:
    import tomllib
except ImportError:  # pragma: no cover - Python 3.10 compatibility
    import tomli as tomllib

import pytest

from codebase_graph.cli import main as cli_main
from codebase_graph.db import LadybugUnavailableError
from codebase_graph.mcp.runtime import runtime_config
from codebase_graph.mcp.server import McpGraphServer, handle_tool_call
from codebase_graph.setup import SetupError, SetupOptions, run_setup
from codebase_graph.setup.instructions import END_MARKER, START_MARKER, upsert_instruction_block
from codebase_graph.setup.mcp_config import configure_mcp_client, server_entry
from codebase_graph.setup.state import build_setup_config, derive_setup_paths, load_setup_config, write_setup_config
from codebase_graph.version import rust_package_version


def test_setup_cli_creates_state_db_mcp_config_instructions_and_searchable_docs(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    mcp_config_path = tmp_path / "config.toml"

    exit_code = cli_main(
        [
            "setup",
            "--repo-root",
            repo_root.as_posix(),
            "--mcp-client",
            "codex",
            "--mcp-config-path",
            mcp_config_path.as_posix(),
        ]
    )
    first_output = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert first_output["state_dir"] == (repo_root / ".codebaseGraph").as_posix()
    assert first_output["db_path"] == (repo_root / ".codebaseGraph" / "fresh_repo_graph.ldb").as_posix()
    assert Path(first_output["db_path"]).exists()
    assert Path(first_output["config_path"]).exists()
    assert first_output["materialization"]["rebuilt"] == 4
    assert first_output["instructions"]["path"] == (repo_root / "AGENTS.md").as_posix()
    assert first_output["mcp_config"]["action"] == "created"

    agents_text = (repo_root / "AGENTS.md").read_text(encoding="utf-8")
    assert agents_text.count(START_MARKER) == 1
    assert agents_text.count(END_MARKER) == 1
    assert "Prefer the `codebase_graph` MCP server tools" in agents_text
    assert "MCP `graph_search`" in agents_text
    assert "MCP `graph_context`" in agents_text
    assert "MCP `graph_architecture_queries`" in agents_text
    assert "MCP `graph_query`" in agents_text
    assert "MCP `graph_schema`" in agents_text
    assert "`graph_query_helpers`" in agents_text
    assert "If MCP tools are unavailable, fall back to CLI" in agents_text
    assert "graph-search" in agents_text
    assert "graph-context" in agents_text
    assert "--format block" not in agents_text
    assert re.search(r"graph-search .*--json", agents_text) is None
    assert re.search(r"graph-context .*--json", agents_text) is None
    assert 'output_format: "json"' in agents_text
    assert "include_structured_content: true" in agents_text
    assert "AI agents receive block output by default for graph CLI and MCP tools" in agents_text
    assert "graph-architecture-queries" in agents_text
    assert (
        "It is prohibited to read the code source before you find the target files using the graph."
        in agents_text
    )
    mcp_payload = tomllib.loads(mcp_config_path.read_text(encoding="utf-8"))
    assert "otherServer" not in mcp_payload.get("mcp_servers", {})
    assert mcp_payload["mcp_servers"]["codebase_graph"]["args"] == [
        "mcp",
        "serve",
        "--config",
        (repo_root / ".codebaseGraph" / "config.json").as_posix(),
    ]

    second_exit_code = cli_main(
        [
            "setup",
            "--repo-root",
            repo_root.as_posix(),
            "--mcp-config-path",
            mcp_config_path.as_posix(),
        ]
    )
    second_output = json.loads(capsys.readouterr().out)

    assert second_exit_code == 0
    assert second_output["config_action"] == "unchanged"
    assert second_output["instructions"]["action"] == "unchanged"
    assert second_output["mcp_config"]["action"] == "unchanged"
    assert (repo_root / "AGENTS.md").read_text(encoding="utf-8").count(START_MARKER) == 1

    server = McpGraphServer.from_paths(config_path=repo_root / ".codebaseGraph" / "config.json")
    docs_payload = handle_tool_call(
        "graph_search",
        {"query": "codebaseGraph workflow", "profile": "docs", "limit": 5},
        runtime=server.runtime,
    )
    health_payload = handle_tool_call("graph_health", {}, runtime=server.runtime)
    symbol_payload = handle_tool_call(
        "graph_search",
        {"query": "SampleService", "profile": "brief", "limit": 3},
        runtime=server.runtime,
    )

    assert health_payload["ok"] is True
    assert health_payload["graph_readable"] is True
    assert health_payload["total_nodes"] > 0
    assert any(hit["path"] == "AGENTS.md" for hit in docs_payload["results"])
    assert any(hit["label"] == "SampleService" for hit in symbol_payload["results"])


def test_claude_instruction_target_uses_block_format(tmp_path: Path) -> None:
    repo_root = tmp_path / "fresh_repo"
    repo_root.mkdir()

    result = upsert_instruction_block(
        repo_root,
        target="claude",
        server_name="codebase_graph",
        config_path=repo_root / ".codebaseGraph" / "config.json",
    )
    claude_text = (repo_root / "CLAUDE.md").read_text(encoding="utf-8")

    assert result.action == "created"
    assert result.path == (repo_root / "CLAUDE.md").as_posix()
    assert not (repo_root / "AGENTS.md").exists()
    assert "Prefer the `codebase_graph` MCP server tools" in claude_text
    assert "MCP `graph_search`" in claude_text
    assert "If MCP tools are unavailable, fall back to CLI" in claude_text
    assert 'output_format: "json"' in claude_text
    assert "include_structured_content: true" in claude_text
    assert "--format block" not in claude_text
    assert re.search(r"graph-search .*--json", claude_text) is None
    assert re.search(r"graph-context .*--json", claude_text) is None


def test_mcp_config_dry_run_preserves_existing_json_servers(tmp_path: Path) -> None:
    config_path = tmp_path / "mcp.json"
    config_path.write_text(
        json.dumps({"mcpServers": {"otherServer": {"command": "other", "args": []}}}),
        encoding="utf-8",
    )
    setup_config_path = tmp_path / ".codebaseGraph" / "config.json"

    dry_run = configure_mcp_client(
        client="generic",
        config_path=config_path,
        setup_config_path=setup_config_path,
        dry_run=True,
    )

    assert dry_run.action == "dry_run"
    assert "codebase_graph" not in json.loads(config_path.read_text(encoding="utf-8"))["mcpServers"]

    written = configure_mcp_client(
        client="generic",
        config_path=config_path,
        setup_config_path=setup_config_path,
    )
    payload = json.loads(config_path.read_text(encoding="utf-8"))

    assert written.action == "created"
    assert set(payload["mcpServers"]) == {"otherServer", "codebase_graph"}


def test_server_entry_prefers_explicit_native_cli(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    native_binary = tmp_path / "bin" / "codebase-graph"
    native_binary.parent.mkdir(parents=True)
    native_binary.write_text("", encoding="utf-8")
    native_binary.chmod(0o755)
    monkeypatch.setenv("CODEBASE_GRAPH_NATIVE_CLI", native_binary.as_posix())
    monkeypatch.setenv("PATH", "")

    entry = server_entry(tmp_path / ".codebaseGraph" / "config.json")

    assert entry["command"] == native_binary.as_posix()


def test_descriptor_does_not_resolve_python_sibling_script() -> None:
    text = Path("src/codebase_graph/setup/descriptor.py").read_text(encoding="utf-8")

    assert "sys.executable" not in text
    assert "with_name(\"codebase-graph\")" not in text


def test_setup_preflight_failure_stops_before_state_creation(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    repo_root = _fresh_repo(tmp_path)

    def fail_preflight() -> None:
        raise LadybugUnavailableError("missing LadyBugDB")

    monkeypatch.setattr("codebase_graph.setup.orchestrator.validate_ladybug_runtime", fail_preflight)

    with pytest.raises(SetupError, match="missing LadyBugDB"):
        run_setup(SetupOptions(repo_root=repo_root, mcp_client="none"))

    assert not (repo_root / ".codebaseGraph").exists()


def test_setup_rejects_state_directory_as_repo_root(tmp_path: Path) -> None:
    state_root = tmp_path / ".codebaseGraph"
    state_root.mkdir()

    with pytest.raises(SetupError, match="state directory"):
        run_setup(SetupOptions(repo_root=state_root, mcp_client="none"))

    assert not (state_root / ".codebaseGraph").exists()


def test_setup_dry_run_does_not_write_repo_or_client_state(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    mcp_config_path = tmp_path / "config.toml"

    exit_code = cli_main(
        [
            "setup",
            "--repo-root",
            repo_root.as_posix(),
            "--mcp-client",
            "codex",
            "--mcp-config-path",
            mcp_config_path.as_posix(),
            "--dry-run",
        ]
    )
    payload = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert payload["config_action"] == "dry_run"
    assert payload["materialization"]["mode"] == "dry_run"
    assert payload["instructions"]["action"] == "dry_run"
    assert payload["mcp_config"]["action"] == "dry_run"
    assert not (repo_root / ".codebaseGraph").exists()
    assert not (repo_root / "AGENTS.md").exists()
    assert not mcp_config_path.exists()


def test_setup_dry_run_accepts_github_copilot_client(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)

    exit_code = cli_main(
        [
            "setup",
            "--repo-root",
            repo_root.as_posix(),
            "--mcp-client",
            "github-copilot",
            "--instructions-target",
            "skip",
            "--dry-run",
        ]
    )
    payload = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert payload["mcp_config"]["action"] == "dry_run"
    assert payload["mcp_config"]["method"] == "file_adapter"
    assert payload["mcp_config"]["path"] == (repo_root / ".vscode" / "mcp.json").as_posix()
    assert not (repo_root / ".vscode" / "mcp.json").exists()


def test_setup_materialization_failure_rolls_back_published_control_files(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)

    def fail_materialize(self: object, *, mode: str = "changed") -> object:
        raise RuntimeError("materialization failed")

    monkeypatch.setattr("codebase_graph.setup.orchestrator.GraphMaterializer.materialize", fail_materialize)

    with pytest.raises(SetupError, match="materialization failed"):
        run_setup(SetupOptions(repo_root=repo_root, mcp_client="none"))

    assert not (repo_root / ".codebaseGraph").exists()
    assert not (repo_root / "AGENTS.md").exists()


def test_mcp_graph_query_rejects_write_like_statements(tmp_path: Path) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    result = run_setup(SetupOptions(repo_root=repo_root, mcp_client="none", instructions_target="skip"))
    server = McpGraphServer.from_paths(config_path=result.paths.config_path)

    with pytest.raises(ValueError, match="read-only"):
        handle_tool_call(
            "graph_query",
            {"statement": "MATCH (n) DELETE n"},
            runtime=server.runtime,
        )


def test_setup_invalid_repo_root_exits_nonzero(tmp_path: Path) -> None:
    missing = tmp_path / "missing"

    with pytest.raises(SystemExit) as exc_info:
        cli_main(["setup", "--repo-root", missing.as_posix(), "--mcp-client", "none"])

    assert exc_info.value.code == 2


def test_runtime_config_uses_repo_root_from_setup_config(tmp_path: Path) -> None:
    repo_root = _fresh_repo(tmp_path)
    paths = derive_setup_paths(repo_root)
    payload = build_setup_config(paths, mcp_command=["codebase-graph", "mcp", "serve", "--config", paths.config_path.as_posix()])

    assert payload["package_version"] == rust_package_version()

    write_setup_config(paths.config_path, payload)
    paths.db_path.write_text("", encoding="utf-8")
    paths.manifest_path.write_text("{}", encoding="utf-8")
    other_root = tmp_path / "other_repo"
    other_root.mkdir()

    runtime = runtime_config(repo_root=other_root, config_path=paths.config_path, db_path=None, manifest_path=None)

    assert runtime.repo_root == repo_root.resolve()
    assert runtime.db_path == paths.db_path
    assert runtime.manifest_path == paths.manifest_path


def test_runtime_config_loads_custom_context_profiles(tmp_path: Path) -> None:
    repo_root = _fresh_repo(tmp_path)
    paths = derive_setup_paths(repo_root)
    payload = build_setup_config(paths, mcp_command=["codebase-graph", "mcp", "serve", "--config", paths.config_path.as_posix()])
    payload["context_profiles"] = {
        "repo_flow": {
            "description": "Repository-specific flow profile.",
            "relations": ["Defines", "Calls"],
            "max_depth": 2,
        }
    }
    write_setup_config(paths.config_path, payload)
    paths.db_path.write_text("", encoding="utf-8")
    paths.manifest_path.write_text("{}", encoding="utf-8")

    runtime = runtime_config(repo_root=repo_root, config_path=paths.config_path, db_path=None, manifest_path=None)

    assert runtime.context_profiles["repo_flow"]["source"] == "repo"
    assert runtime.context_profiles["repo_flow"]["relations"] == ["Defines", "Calls"]
    assert runtime.context_profiles["change_impact"]["source"] == "builtin"
    assert "graph_impact" not in runtime.context_profiles


def test_setup_config_rejects_invalid_custom_context_profile(tmp_path: Path) -> None:
    repo_root = _fresh_repo(tmp_path)
    paths = derive_setup_paths(repo_root)
    payload = build_setup_config(paths, mcp_command=["codebase-graph", "mcp", "serve", "--config", paths.config_path.as_posix()])
    payload["context_profiles"] = {
        "bad_profile": {
            "description": "Invalid profile.",
            "relations": ["MissingRelation"],
            "max_depth": 1,
        }
    }
    paths.config_path.parent.mkdir(parents=True)
    paths.config_path.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(ValueError, match="unknown relation"):
        load_setup_config(paths.config_path)


def test_setup_config_rejects_database_path_outside_state_dir(tmp_path: Path) -> None:
    repo_root = _fresh_repo(tmp_path)
    paths = derive_setup_paths(repo_root)
    payload = build_setup_config(paths, mcp_command=["codebase-graph", "mcp", "serve", "--config", paths.config_path.as_posix()])
    payload["database_path"] = (tmp_path / "other.ldb").as_posix()
    paths.config_path.parent.mkdir(parents=True)
    paths.config_path.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(ValueError, match="database_path must be"):
        load_setup_config(paths.config_path)


def test_pyproject_is_tool_config_without_python_package_metadata() -> None:
    payload = tomllib.loads(Path("pyproject.toml").read_text(encoding="utf-8"))
    dev_requirements = Path("requirements-dev.txt").read_text(encoding="utf-8")

    assert "real_ladybug>=0.15.3,<0.16" in dev_requirements
    assert "tree-sitter>=0.25.2,<0.26" in dev_requirements
    assert "tree-sitter-python>=0.25.0,<0.26" in dev_requirements
    assert "build-system" not in payload
    assert "project" not in payload
    assert "maturin" not in payload["tool"]
    assert "ruff" in payload["tool"]
    assert "pytest" in payload["tool"]


def _fresh_repo(tmp_path: Path) -> Path:
    repo_root = tmp_path / "fresh_repo"
    package = repo_root / "sample_project"
    package.mkdir(parents=True)
    (package / "__init__.py").write_text("", encoding="utf-8")
    (package / "service.py").write_text(
        "class SampleService:\n"
        "    def run(self) -> str:\n"
        "        return helper()\n\n"
        "def helper() -> str:\n"
        "    return 'ok'\n",
        encoding="utf-8",
    )
    (repo_root / "README.md").write_text(
        "# Fresh Repo\n\nThis repository documents the SampleService workflow.\n",
        encoding="utf-8",
    )
    return repo_root
