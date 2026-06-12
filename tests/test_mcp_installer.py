from __future__ import annotations

import json
import subprocess
from pathlib import Path

import pytest

from codebase_graph.cli import main as cli_main
from codebase_graph.setup.clients import get_client_adapter
from codebase_graph.setup.descriptor import build_server_descriptor
from codebase_graph.setup.installer import (
    INSTALL_CLIENTS,
    INSTALL_STRATEGIES,
    McpInstallOptions,
    default_server_name,
    install_mcp_clients,
    install_mcp_server,
)
from codebase_graph.setup.state import build_setup_config, derive_setup_paths, write_setup_config


def test_default_server_name_is_namespace_safe() -> None:
    assert default_server_name("My Service") == "codebase_graph_my_service"


def test_install_strategy_registry_covers_advertised_clients() -> None:
    assert set(INSTALL_CLIENTS) == set(INSTALL_STRATEGIES)
    assert {"github-copilot", "copilot-studio", "microsoft-copilot"}.issubset(INSTALL_CLIENTS)
    for client, strategy in INSTALL_STRATEGIES.items():
        assert strategy.adapter_client_id("local")
        if strategy.native_command_builder is not None:
            assert strategy.native_executable
        if client == "claude":
            assert strategy.adapter_client_id("project") == "claude-project"
        if client == "claude-project":
            assert strategy.install_scope("local") == "project"


def test_codex_native_command_generation_uses_repo_server_name(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh repo")
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    result = install_mcp_server(McpInstallOptions(setup_config_path=config_path, dry_run=True))

    assert result.action == "dry_run"
    assert result.method == "native_cli"
    assert result.server_name == "codebase_graph_fresh_repo"
    assert result.command[:4] == ["codex", "mcp", "add", "codebase_graph_fresh_repo"]
    assert result.command[4] == "--"


def test_claude_native_command_includes_transport_and_scope(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    result = install_mcp_server(
        McpInstallOptions(client="claude", scope="user", setup_config_path=config_path, dry_run=True)
    )

    assert result.command[:8] == [
        "claude",
        "mcp",
        "add",
        "--transport",
        "stdio",
        "--scope",
        "user",
        "codebase_graph_fresh_repo",
    ]


def test_claude_project_native_command_forces_project_scope(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    result = install_mcp_server(
        McpInstallOptions(client="claude-project", scope="user", setup_config_path=config_path, dry_run=True)
    )

    assert result.command[6:8] == ["project", "codebase_graph_fresh_repo"]


def test_openclaw_native_command_emits_server_json(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    result = install_mcp_server(
        McpInstallOptions(client="openclaw", setup_config_path=config_path, dry_run=True)
    )
    entry = json.loads(result.command[-1])

    assert result.command[:4] == ["openclaw", "mcp", "set", "codebase_graph_fresh_repo"]
    assert entry["type"] == "stdio"
    assert entry["args"][:2] == ["mcp", "serve"]


def test_missing_native_cli_falls_back_to_file_adapter(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    codex_home = tmp_path / "codex-home"
    monkeypatch.setenv("CODEX_HOME", codex_home.as_posix())
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: None)

    result = install_mcp_server(McpInstallOptions(client="codex", setup_config_path=config_path))

    assert result.action == "created"
    assert result.method == "file_adapter"
    assert result.path == (codex_home / "config.toml").as_posix()
    assert "executable not found" in result.native_error
    assert (codex_home / "config.toml").exists()


def test_native_cli_failure_falls_back_to_adapter(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    codex_home = tmp_path / "codex-home"
    monkeypatch.setenv("CODEX_HOME", codex_home.as_posix())
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    def fail_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess(command, 2, stdout="", stderr="native failed")

    monkeypatch.setattr("codebase_graph.setup.installer.subprocess.run", fail_run)

    result = install_mcp_server(McpInstallOptions(client="codex", setup_config_path=config_path))

    assert result.action == "created"
    assert result.method == "file_adapter"
    assert result.native_command[:4] == ["codex", "mcp", "add", "codebase_graph_fresh_repo"]
    assert result.native_error == "exit 2: native failed"
    assert (codex_home / "config.toml").exists()


def test_dry_run_never_writes_files_or_calls_native_cli(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setenv("HOME", tmp_path.as_posix())

    def fail_run(*args: object, **kwargs: object) -> subprocess.CompletedProcess[str]:
        raise AssertionError("dry-run must not call subprocess.run")

    monkeypatch.setattr("codebase_graph.setup.installer.subprocess.run", fail_run)

    result = install_mcp_server(
        McpInstallOptions(client="generic", setup_config_path=config_path, dry_run=True)
    )

    assert result.action == "dry_run"
    assert result.method == "file_adapter"
    assert not (tmp_path / ".config" / "mcp" / "mcp.json").exists()


def test_setup_compatibility_uses_snake_case_server_name_and_file_adapter(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    pytest.importorskip("tree_sitter")
    pytest.importorskip("tree_sitter_python")
    pytest.importorskip("real_ladybug")
    repo_root = _fresh_repo(tmp_path)
    mcp_config_path = tmp_path / "codex.toml"
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    exit_code = cli_main(
        [
            "setup",
            "--repo-root",
            repo_root.as_posix(),
            "--mcp-client",
            "codex",
            "--mcp-config-path",
            mcp_config_path.as_posix(),
            "--instructions-target",
            "skip",
        ]
    )
    output = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert output["mcp_config"]["server_name"] == "codebase_graph"
    assert output["mcp_config"]["method"] == "file_adapter"
    assert output["mcp_config"]["path"] == mcp_config_path.as_posix()


def test_hermes_default_path_is_documented_home_config(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(Path, "home", classmethod(lambda cls: tmp_path))
    descriptor = build_server_descriptor(tmp_path / ".codebaseGraph" / "config.json")

    assert get_client_adapter("hermes").default_config_path(descriptor) == tmp_path / ".hermes" / "config.yaml"


def test_github_copilot_default_path_is_vscode_workspace_config(tmp_path: Path) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    descriptor = build_server_descriptor(config_path, repo_root=tmp_path / "fresh_repo")

    assert get_client_adapter("github-copilot").default_config_path(descriptor) == (
        tmp_path / "fresh_repo" / ".vscode" / "mcp.json"
    )


def test_unsupported_install_client_lists_copilot_clients(tmp_path: Path) -> None:
    with pytest.raises(ValueError) as exc_info:
        install_mcp_server(
            McpInstallOptions(
                client="missing",
                setup_config_path=tmp_path / ".codebaseGraph" / "config.json",
                require_setup_config=False,
            )
        )

    message = str(exc_info.value)
    assert "github-copilot" in message
    assert "copilot-studio" in message
    assert "microsoft-copilot" in message


def test_all_client_install_reports_partial_failure(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import codebase_graph.setup.installer as installer

    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setenv("CODEX_HOME", (tmp_path / "codex-home").as_posix())
    monkeypatch.setattr(installer, "INSTALL_CLIENTS", ("codex", "generic"))
    monkeypatch.setattr(installer.shutil, "which", lambda name: None)
    original_get_adapter = installer.get_client_adapter

    def get_adapter(client: str) -> object:
        if client == "generic":
            raise ValueError("adapter unavailable")
        return original_get_adapter(client)

    monkeypatch.setattr(installer, "get_client_adapter", get_adapter)

    results = install_mcp_clients(McpInstallOptions(client="all", setup_config_path=config_path))

    assert [result.client for result in results] == ["codex", "generic"]
    assert results[0].action == "created"
    assert results[1].action == "failed"
    assert results[1].error == "adapter unavailable"


def test_mcp_install_cli_dry_run_json(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    monkeypatch.setattr("codebase_graph.setup.installer.shutil.which", lambda name: f"/usr/bin/{name}")

    exit_code = cli_main(
        ["mcp", "install", "--client", "codex", "--config-path", config_path.as_posix(), "--dry-run", "--json"]
    )
    output = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert output["action"] == "dry_run"
    assert output["method"] == "native_cli"
    assert output["server_name"] == default_server_name("fresh_repo")


def test_mcp_install_cli_writes_github_copilot_workspace_config(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    repo_root = tmp_path / "fresh_repo"
    config_path = _write_setup_config(repo_root)

    exit_code = cli_main(
        ["mcp", "install", "--client", "github-copilot", "--config-path", config_path.as_posix(), "--json"]
    )
    output = json.loads(capsys.readouterr().out)
    config_payload = json.loads((repo_root / ".vscode" / "mcp.json").read_text(encoding="utf-8"))

    assert exit_code == 0
    assert output["action"] == "created"
    assert output["method"] == "file_adapter"
    assert output["path"] == (repo_root / ".vscode" / "mcp.json").as_posix()
    assert config_payload["servers"]["codebase_graph_fresh_repo"]["type"] == "stdio"
    assert config_payload["servers"]["codebase_graph_fresh_repo"]["args"][0:2] == ["mcp", "serve"]


def test_mcp_install_cli_reports_copilot_studio_metadata(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")

    exit_code = cli_main(
        ["mcp", "install", "--client", "copilot-studio", "--config-path", config_path.as_posix(), "--json"]
    )
    output = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert output["action"] == "reported"
    assert output["method"] == "manual_metadata"
    assert output["path"] is None
    assert output["payload"]["http"]["url"] == "http://127.0.0.1:8765/mcp"
    assert output["payload"]["http"]["start_command"][1:4] == ["mcp", "http", "--config"]
    assert output["payload"]["stdio"]["type"] == "stdio"


def test_mcp_install_cli_accepts_client_config_path(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    config_path = _write_setup_config(tmp_path / "fresh_repo")
    client_config_path = tmp_path / "client" / "mcp.json"

    exit_code = cli_main(
        [
            "mcp",
            "install",
            "--client",
            "generic",
            "--config-path",
            config_path.as_posix(),
            "--client-config-path",
            client_config_path.as_posix(),
            "--json",
        ]
    )
    output = json.loads(capsys.readouterr().out)

    assert exit_code == 0
    assert output["path"] == client_config_path.as_posix()
    assert client_config_path.exists()


def _write_setup_config(repo_root: Path) -> Path:
    repo_root.mkdir(parents=True)
    paths = derive_setup_paths(repo_root)
    mcp_command = ["codebase-graph", "mcp", "serve", "--config", paths.config_path.as_posix()]
    payload = build_setup_config(paths, mcp_command=mcp_command)
    write_setup_config(paths.config_path, payload)
    return paths.config_path


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
    return repo_root
