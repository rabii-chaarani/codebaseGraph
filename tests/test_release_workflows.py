from __future__ import annotations

import io
import json
import re
import subprocess
from pathlib import Path
from typing import Any

import pytest

from scripts import smoke_native_artifact
from scripts import check_release_gate
from scripts.check_release_gate import (
    RELEASE_CONFIRMATION_FLAGS,
    SUPPORTED_ARTIFACT_SUFFIXES,
    _jobs_missing_timeout,
    _workflow_action_pin_issues,
    run_checks,
)


WORKFLOWS = (
    Path(".github/workflows/ci.yml"),
    Path(".github/workflows/release.yml"),
)


def test_github_actions_are_pinned_to_immutable_commits() -> None:
    for path in WORKFLOWS:
        mutable_refs = _workflow_action_pin_issues(path, path.read_text(encoding="utf-8"))

        assert mutable_refs == []


def test_action_pin_checker_rejects_bare_external_actions() -> None:
    text = """
jobs:
  lint:
    steps:
      - uses: actions/checkout
      - uses: ./.github/actions/local-smoke
      - uses: actions/setup-python@a26af69be951a213d495a4c3e4e4022e16d87065
"""

    issues = _workflow_action_pin_issues(Path(".github/workflows/example.yml"), text)

    assert [issue.code for issue in issues] == ["workflow-action-not-pinned"]
    assert "actions/checkout" in issues[0].message


def test_release_workflows_smoke_test_native_archives() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")
        assert 'MACOSX_DEPLOYMENT_TARGET: "13.3"' in text
        assert "cargo build --manifest-path rust/Cargo.toml --locked --release --bin codebase-graph" in text
        assert ".tar.gz" in text
        assert 'matrix.binary' in text
        assert "graph-schema --json" in text
        assert "setup --repo-root tests/fixtures/sample_project" in text
        assert "scripts/smoke_native_artifact.py" in text
        for suffix in SUPPORTED_ARTIFACT_SUFFIXES:
            assert suffix in text
        assert "pypa/gh-action-pypi-publish" not in text
        assert "python -m maturin build --release" not in text
        assert "python -m build" not in text
    assert "codebase-graph-${" in Path(".github/workflows/release.yml").read_text(encoding="utf-8")


def test_package_smoke_uses_newline_delimited_mcp_stdio() -> None:
    stdout = io.BytesIO(b'{"id":1,"jsonrpc":"2.0","result":{}}\n')
    stdin = io.BytesIO()

    response = smoke_native_artifact._rpc(stdin, stdout, "initialize", {"protocolVersion": "2025-11-25"})

    assert response == {"id": 1, "jsonrpc": "2.0", "result": {}}
    assert stdin.getvalue().endswith(b"\n")
    assert b"Content-Length" not in stdin.getvalue()


def test_package_smoke_requests_json_graph_health(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    commands: list[list[str]] = []

    def fake_run(command: list[str]) -> subprocess.CompletedProcess[str]:
        commands.append(command)
        if "setup" in command:
            if "--dry-run" in command:
                return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"config_action": "dry_run"}))
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"config_path": str(tmp_path / "config.json")}))
        if command == ["/tmp/codebase-graph", "--help"]:
            return subprocess.CompletedProcess(command, 0, stdout="codebase-graph native CLI\n")
        if "graph-schema" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"node_types": [{}], "relation_types": [{}]}))
        if "materialize" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"database_written": True}))
        if "graph-health" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"ok": True, "graph_readable": True}))
        if "graph-search" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"results": [{"id": "Class:SampleService"}]}))
        raise AssertionError(command)

    def fake_run_unchecked(command: list[str]) -> subprocess.CompletedProcess[str]:
        commands.append(command)
        if command == ["/tmp/codebase-graph", "legacy-protocol"]:
            return subprocess.CompletedProcess(command, 1, stdout="", stderr="legacy-protocol is test-only")
        raise AssertionError(command)

    monkeypatch.setattr(smoke_native_artifact, "_run", fake_run)
    monkeypatch.setattr(smoke_native_artifact, "_run_unchecked", fake_run_unchecked)
    monkeypatch.setattr(smoke_native_artifact, "_install_verify_smoke", lambda *args: None)
    monkeypatch.setattr(smoke_native_artifact, "_mcp_smoke", lambda command: None)

    assert smoke_native_artifact.main(["smoke_native_artifact.py", "/tmp/codebase-graph"]) == 0

    health_command = next(command for command in commands if "graph-health" in command)
    assert "--json" in health_command
    assert any(command[:2] == ["/tmp/codebase-graph", "graph-schema"] and "--json" in command for command in commands)
    assert any(command[:2] == ["/tmp/codebase-graph", "materialize"] for command in commands)
    assert ["/tmp/codebase-graph", "--help"] in commands
    assert ["/tmp/codebase-graph", "legacy-protocol"] in commands


def test_package_smoke_uses_rust_binary_mcp_subcommand(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    executable = tmp_path / "codebase-graph"
    executable.touch()
    mcp_commands: list[list[str]] = []

    def fake_run(command: list[str]) -> subprocess.CompletedProcess[str]:
        if "setup" in command:
            if "--dry-run" in command:
                return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"config_action": "dry_run"}))
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"config_path": str(tmp_path / "config.json")}))
        if command == [executable.as_posix(), "--help"]:
            return subprocess.CompletedProcess(command, 0, stdout="codebase-graph native CLI\n")
        if "graph-schema" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"node_types": [{}], "relation_types": [{}]}))
        if "materialize" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"database_written": True}))
        if "graph-health" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"ok": True, "graph_readable": True}))
        if "graph-search" in command:
            return subprocess.CompletedProcess(command, 0, stdout=json.dumps({"results": [{"id": "Class:SampleService"}]}))
        raise AssertionError(command)

    def fake_run_unchecked(command: list[str]) -> subprocess.CompletedProcess[str]:
        if command == [executable.as_posix(), "legacy-protocol"]:
            return subprocess.CompletedProcess(command, 1, stdout="", stderr="legacy-protocol is test-only")
        raise AssertionError(command)

    monkeypatch.setattr(smoke_native_artifact, "_run", fake_run)
    monkeypatch.setattr(smoke_native_artifact, "_run_unchecked", fake_run_unchecked)
    monkeypatch.setattr(smoke_native_artifact, "_install_verify_smoke", lambda *args: None)
    monkeypatch.setattr(smoke_native_artifact, "_mcp_smoke", lambda command: mcp_commands.append(command))

    assert smoke_native_artifact.main(["smoke_native_artifact.py", executable.as_posix()]) == 0

    assert [command[:2] for command in mcp_commands] == [[executable.as_posix(), "mcp"]]


def test_package_smoke_rejects_legacy_mcp_sidecar(tmp_path: Path) -> None:
    executable = tmp_path / "codebase-graph"
    executable.touch()
    (tmp_path / "codebase-graph-mcp").touch()

    try:
        smoke_native_artifact.main(["smoke_native_artifact.py", executable.as_posix()])
    except AssertionError as exc:
        assert "legacy MCP sidecar must not be shipped" in str(exc)
    else:  # pragma: no cover - defensive assertion for the no-sidecar contract.
        raise AssertionError("native artifact smoke accepted codebase-graph-mcp")


def test_package_mcp_smoke_requests_structured_health(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[tuple[str, dict[str, Any]]] = []

    class FakeProcess:
        def __init__(self, *_args: Any, **_kwargs: Any) -> None:
            self.stdin = io.BytesIO()
            self.stdout = io.BytesIO()
            self.stderr = io.BytesIO()
            self.returncode = 0

        def wait(self, *, timeout: int) -> int:
            return self.returncode

    def fake_rpc(_stdin: Any, _stdout: Any, method: str, params: dict[str, Any]) -> dict[str, Any]:
        calls.append((method, params))
        if method == "initialize":
            return {"result": {"protocolVersion": "2025-11-25"}}
        if method == "tools/list":
            return {"result": {"tools": [{"name": "graph_health"}, {"name": "graph_search"}, {"name": "graph_query"}]}}
        if method == "tools/call":
            return {"result": {"structuredContent": {"ok": True}}}
        raise AssertionError(method)

    monkeypatch.setattr(smoke_native_artifact.subprocess, "Popen", FakeProcess)
    monkeypatch.setattr(smoke_native_artifact, "_rpc", fake_rpc)

    smoke_native_artifact._mcp_smoke(["/tmp/codebase-graph", "mcp", "serve"])

    assert ("tools/call", {"name": "graph_health", "arguments": {"include_structured_content": True}}) in calls


def test_release_workflow_enforces_production_gate_before_build() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "production-gate:" in text
    assert "python scripts/check_release_gate.py" in text
    assert "--production" in text
    assert "build:\n    name: build native release artifacts (${{ matrix.os }})\n    needs:\n      - release-please\n      - production-gate" in text


def test_release_workflow_does_not_publish_to_pypi() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "pypi-environment-smoke" not in text
    assert "publish-pypi" not in text
    assert "publish existing release to PyPI" not in text
    assert "pypa/gh-action-pypi-publish" not in text
    assert "name: pypi" not in text


def test_release_please_is_skipped_during_existing_tag_verification() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "release-please:\n    name: release please" in text
    assert "inputs.publish-existing-tag == ''" in text


def test_release_workflow_can_verify_existing_release_tag_native_artifacts() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "publish-existing-tag:" in text
    assert "existing release gate" in text
    assert "verify existing native artifacts" in text
    assert "inputs.publish-existing-tag != ''" in text
    assert 'RELEASE_TAG: ${{ inputs.publish-existing-tag }}' in text
    assert 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --dir dist' in text
    assert '--pattern "codebase-graph-*.tar.gz"' in text
    assert '--pattern "codebase-graph-*.tar.gz.sha256"' in text
    for suffix in SUPPORTED_ARTIFACT_SUFFIXES:
        assert suffix in text
    assert "release tag must match vX.Y.Z" in text


def test_release_please_uses_strict_semver_tags() -> None:
    config = json.loads(Path("release-please-config.json").read_text(encoding="utf-8"))
    root_package = config["packages"]["."]

    assert root_package["include-v-in-tag"] is True
    assert root_package["include-component-in-tag"] is False
    assert "release tag must match vX.Y.Z" in Path(".github/workflows/release.yml").read_text(encoding="utf-8")
    assert "component-prefixed tags" in Path("docs/release.md").read_text(encoding="utf-8")


def test_conda_recipe_builds_native_binary_without_python_runtime() -> None:
    text = Path("conda-forge/recipe/meta.yaml").read_text(encoding="utf-8")

    assert "https://github.com/rabii-chaarani/codebaseGraph/archive/refs/tags/v{{ version }}.tar.gz" in text
    assert "PUT_RELEASE_ARCHIVE_SHA256_HERE" in text
    assert "cargo build --manifest-path rust/Cargo.toml --locked --release --bin codebase-graph" in text
    assert 'cp rust/target/release/codebase-graph "${PREFIX}/bin/codebase-graph"' in text
    assert "codebase-graph mcp --help" in text
    assert "codebase-graph-mcp" not in text
    assert "pypi.org" not in text
    assert "pypi_name" not in text
    assert "maturin" not in text
    assert "python" not in text.lower()
    assert "cargo-bundle-licenses" in text
    assert "license: MIT" in text
    assert "PUT_SPDX_LICENSE_ID_HERE" not in text


def test_hosted_workflows_run_real_vulnerability_scans() -> None:
    text = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
    assert "pip_audit --strict" in text
    assert "pip_audit --strict -r requirements-dev.txt" in text
    assert "--skip-editable" not in text
    assert re.search(r"pip_audit\b[^\n]*--dry-run", text) is None


def test_supply_chain_workflow_audits_project_dependencies() -> None:
    text = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
    match = re.search(r"  supply-chain:\n(?P<body>.*?)(?=\n  [A-Za-z0-9_-]+:|\Z)", text, re.DOTALL)

    assert match is not None
    body = match.group("body")
    assert "python -m pip install -r requirements-dev.txt" in body
    assert '".[dev]"' not in body
    assert "python -m pip_audit --strict -r requirements-dev.txt" in body
    assert "--pyproject pyproject.toml" not in body


def test_project_metadata_is_dev_harness_not_production_package_surface() -> None:
    text = Path("pyproject.toml").read_text(encoding="utf-8")

    assert "[build-system]" not in text
    assert "[project]" not in text
    assert "[project.scripts]" not in text
    assert "[tool.maturin]" not in text
    assert "console_scripts" not in text


def test_rust_crate_owns_production_package_metadata() -> None:
    text = Path("rust/crates/codebase_graph_native/Cargo.toml").read_text(encoding="utf-8")

    assert 'name = "codebase_graph_native"' in text
    assert 'description = "Native codebaseGraph CLI and MCP server for local code knowledge graphs."' in text
    assert 'license = "MIT"' in text
    assert 'repository = "https://github.com/rabii-chaarani/codebaseGraph"' in text
    assert 'readme = "../../../README.md"' in text
    assert 'keywords = ["codebase", "graph", "mcp", "cli", "analysis"]' in text
    assert 'categories = ["command-line-utilities", "development-tools"]' in text
    assert 'name = "codebase-graph"' in text
    assert 'path = "src/bin/codebase-graph.rs"' in text


def test_ci_has_rust_native_gate() -> None:
    text = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")

    assert "rust-native:" in text
    assert "Build Rust CLI for compatibility tests" in text
    assert "cargo build --manifest-path rust/Cargo.toml --locked --bin codebase-graph" in text
    assert "cargo test --manifest-path rust/Cargo.toml --locked" in text
    assert "cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features --locked -- -D warnings" in text
    assert "package:\n    name: native package (${{ matrix.os }})" in text
    for suffix in SUPPORTED_ARTIFACT_SUFFIXES:
        assert suffix in text


def test_security_policy_exists() -> None:
    text = Path("SECURITY.md").read_text(encoding="utf-8")

    assert "Reporting a Vulnerability" in text
    assert "graph_query" in text
    assert "--allow-remote" in text


def test_release_docs_list_production_confirmation_flags() -> None:
    text = Path("docs/release.md").read_text(encoding="utf-8")

    assert "protected `release` GitHub environment" in text
    assert "PyPI" not in text
    for flag in RELEASE_CONFIRMATION_FLAGS:
        env_var = f"CODEBASE_GRAPH_CONFIRM_{flag.upper().replace('-', '_')}"
        assert env_var in text
        assert f"--confirm {flag}" in text
    assert "CODEBASE_GRAPH_REQUIRE_CONDA" in text
    assert "--require-conda" in text


def test_workflow_jobs_have_timeouts() -> None:
    for path in WORKFLOWS:
        missing = _jobs_missing_timeout(path.read_text(encoding="utf-8"))

        assert missing == []


def test_workflows_pin_node24_capable_first_party_actions() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")

        assert "actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10" in text
        assert "actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd" not in text
        assert "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5" not in text
        assert "actions/setup-python@a26af69be951a213d495a4c3e4e4022e16d87065" not in text
    assert "actions/setup-python@a309ff8b426b58ec0e2a45f0f869d46889d02405" in Path(".github/workflows/ci.yml").read_text(
        encoding="utf-8"
    )


def test_workflows_avoid_node20_artifact_actions() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")

        assert "actions/upload-artifact@" not in text
        assert "actions/download-artifact@" not in text


def test_release_workflow_downloads_native_archives_from_github_release() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --dir dist' in text
    for suffix in SUPPORTED_ARTIFACT_SUFFIXES:
        assert suffix in text
    assert "does not include a native {suffix} codebase-graph archive" in text
    assert "does not include checksum" in text


def test_workflows_avoid_hosted_cache_warning_annotations() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")

        assert 'PIP_NO_CACHE_DIR: "1"' in text
        assert "cache: pip" not in text


def test_ci_uses_explicit_windows_runner_label() -> None:
    text = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")

    assert "windows-2022" in text
    assert "windows-latest" not in text


def test_local_release_gate_passes() -> None:
    assert run_checks(production=False, require_conda=False, confirmations=set()) == []


def test_production_release_gate_reports_owner_controlled_blockers() -> None:
    issues = run_checks(production=True, require_conda=True, confirmations=set())
    codes = {issue.code for issue in issues}
    messages = {issue.message for issue in issues}

    assert "license-metadata-missing" not in codes
    assert "license-file-missing" not in codes
    assert "external-confirmation-missing" in codes
    assert "conda-placeholder" in codes
    assert "conda recipe still contains PUT_RELEASE_VERSION_HERE." in messages
    assert "conda recipe still contains PUT_RELEASE_ARCHIVE_SHA256_HERE." in messages
    assert "conda recipe still contains PUT_SPDX_LICENSE_ID_HERE." not in messages


def test_release_gate_rejects_generated_python_package_artifacts(monkeypatch, tmp_path) -> None:
    artifact_dir = tmp_path / "src" / "codebase_graph.egg-info"
    artifact_dir.mkdir(parents=True)
    extension = tmp_path / "src" / "codebase_graph" / "_native" / "_native.cpython-312-darwin.so"
    extension.parent.mkdir(parents=True)
    extension.write_text("", encoding="utf-8")
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_no_python_package_artifacts()

    assert [issue.code for issue in issues] == [
        "python-package-artifact-present",
        "python-package-artifact-present",
    ]
    assert any("src/codebase_graph.egg-info" in issue.message for issue in issues)
    assert any("_native.cpython-312-darwin.so" in issue.message for issue in issues)


def test_release_gate_rejects_python_package_metadata(monkeypatch, tmp_path) -> None:
    pyproject = tmp_path / "pyproject.toml"
    pyproject.write_text(
        '[build-system]\nrequires = ["maturin>=1,<2"]\nbuild-backend = "maturin"\n\n'
        '[project]\nname = "codebase-graph"\n\n'
        '[project.scripts]\ncodebase-graph = "codebase_graph.cli:main"\n\n'
        '[tool.maturin]\nfeatures = ["python-extension"]\n',
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_no_python_package_metadata()

    assert [issue.code for issue in issues] == [
        "python-build-system-present",
        "python-project-metadata-present",
        "python-entrypoint-metadata-present",
        "python-maturin-metadata-present",
    ]


def test_release_gate_allows_python_tool_config_only(monkeypatch, tmp_path) -> None:
    pyproject = tmp_path / "pyproject.toml"
    pyproject.write_text(
        "[tool.ruff]\n"
        "line-length = 120\n\n"
        "[tool.pytest.ini_options]\n"
        'testpaths = ["tests"]\n',
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    assert check_release_gate._check_no_python_package_metadata() == []


def test_release_gate_rejects_python_entrypoint_fallbacks(monkeypatch, tmp_path) -> None:
    cli_path = tmp_path / "src" / "codebase_graph" / "cli" / "__init__.py"
    server_path = tmp_path / "src" / "codebase_graph" / "mcp" / "server.py"
    cli_path.parent.mkdir(parents=True)
    server_path.parent.mkdir(parents=True)
    cli_path.write_text("from codebase_graph.ingest.materializer import GraphMaterializer\n", encoding="utf-8")
    server_path.write_text("from codebase_graph._native.product_cli import run_product_cli_process\n", encoding="utf-8")
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_thin_python_entrypoints()

    assert [issue.code for issue in issues] == [
        "python-entrypoint-fallback-present",
        "python-entrypoint-fallback-present",
        "python-entrypoint-fallback-present",
    ]
    assert any("GraphMaterializer" in issue.message for issue in issues)
    assert any("codebase_graph._native.product_cli" in issue.message for issue in issues)
    assert any("run_product_cli" in issue.message for issue in issues)


def test_release_gate_accepts_thin_python_entrypoint_launchers(monkeypatch, tmp_path) -> None:
    cli_path = tmp_path / "src" / "codebase_graph" / "cli" / "__init__.py"
    server_path = tmp_path / "src" / "codebase_graph" / "mcp" / "server.py"
    cli_path.parent.mkdir(parents=True)
    server_path.parent.mkdir(parents=True)
    cli_path.write_text(
        "from codebase_graph.native_binary import resolve_native_product_binary\n",
        encoding="utf-8",
    )
    server_path.write_text(
        "from codebase_graph.native_binary import resolve_native_product_binary\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    assert check_release_gate._check_thin_python_entrypoints() == []


def test_release_gate_rejects_python_mcp_db_fallback(monkeypatch, tmp_path) -> None:
    tools_path = tmp_path / "src" / "codebase_graph" / "mcp" / "tools.py"
    tools_path.parent.mkdir(parents=True)
    tools_path.write_text(
        "def _native_tool_payload(name, arguments, runtime):\n"
        "    return None\n"
        "def handle_tool_call():\n"
        "    native_payload = _native_tool_payload('graph_search', {}, runtime)\n"
        "    return native_payload\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_no_python_mcp_db_fallbacks()

    assert [issue.code for issue in issues] == [
        "mcp-rust-owned-routing-missing",
        "mcp-python-db-fallback-present",
        "mcp-python-db-fallback-present",
    ]


def test_release_gate_accepts_rust_owned_mcp_db_routing(monkeypatch, tmp_path) -> None:
    tools_path = tmp_path / "src" / "codebase_graph" / "mcp" / "tools.py"
    tools_path.parent.mkdir(parents=True)
    tools_path.write_text(
        "RUST_OWNED_TOOLS = {'graph_health', 'graph_search', 'graph_context', 'graph_query'}\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    assert check_release_gate._check_no_python_mcp_db_fallbacks() == []


def test_release_gate_rejects_native_opt_in_documentation(monkeypatch, tmp_path) -> None:
    (tmp_path / "docs").mkdir()
    (tmp_path / "README.md").write_text("Rust is experimental; set CODEBASE_GRAPH_NATIVE=1.\n", encoding="utf-8")
    (tmp_path / "docs" / "release.md").write_text("Native Rust CLI and MCP entrypoints are required.\n", encoding="utf-8")
    (tmp_path / "docs" / "rust_rewrite.md").write_text(
        "Python keeps ownership of production behavior. Keep outputs comparable until the Python path is removed.\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_rust_only_documentation()

    assert [issue.code for issue in issues] == [
        "rust-native-opt-in-env",
        "rust-experimental-language",
        "python-production-ownership-language",
        "python-test-oracle-language",
    ]


def test_release_gate_accepts_rust_only_documentation(monkeypatch, tmp_path) -> None:
    (tmp_path / "docs").mkdir()
    (tmp_path / "README.md").write_text("The shipped CLI and MCP server are native Rust binaries.\n", encoding="utf-8")
    (tmp_path / "docs" / "release.md").write_text("Native Rust CLI and MCP entrypoints are required.\n", encoding="utf-8")
    (tmp_path / "docs" / "rust_rewrite.md").write_text("Rust owns production CLI and MCP behavior.\n", encoding="utf-8")
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    assert check_release_gate._check_rust_only_documentation() == []


def test_release_gate_rejects_retired_native_feature_gate(monkeypatch, tmp_path) -> None:
    module = tmp_path / "src" / "codebase_graph" / "legacy_toggle.py"
    module.parent.mkdir(parents=True)
    module.write_text('if os.environ.get("CODEBASE_GRAPH_NATIVE") == "1":\n    pass\n', encoding="utf-8")
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_no_retired_native_feature_gate()

    assert [issue.code for issue in issues] == ["retired-native-feature-gate-present"]


def test_release_gate_allows_explicit_native_cli_resolver(monkeypatch, tmp_path) -> None:
    module = tmp_path / "src" / "codebase_graph" / "native_binary.py"
    module.parent.mkdir(parents=True)
    module.write_text('os.environ.get("CODEBASE_GRAPH_NATIVE_CLI")\n', encoding="utf-8")
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    assert check_release_gate._check_no_retired_native_feature_gate() == []


def test_release_gate_reports_missing_release_workflow(monkeypatch, tmp_path) -> None:
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_release_workflow_permissions()

    assert [issue.code for issue in issues] == ["workflow-missing"]
    assert ".github/workflows/release.yml is required." in issues[0].message


def test_release_gate_reports_missing_conda_recipe(monkeypatch, tmp_path) -> None:
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_conda_recipe()

    assert [issue.code for issue in issues] == ["conda-recipe-missing"]
    assert "conda-forge/recipe/meta.yaml is required." in issues[0].message


def test_release_gate_rejects_conda_mcp_sidecar(monkeypatch, tmp_path) -> None:
    recipe = tmp_path / "conda-forge" / "recipe" / "meta.yaml"
    recipe.parent.mkdir(parents=True)
    recipe.write_text(
        "build:\n"
        "  script:\n"
        "    - ln -s ${PREFIX}/bin/codebase-graph ${PREFIX}/bin/codebase-graph-mcp\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(check_release_gate, "REPO_ROOT", tmp_path)

    issues = check_release_gate._check_conda_recipe()

    assert [issue.code for issue in issues] == ["conda-python-mcp-sidecar-forbidden"]
