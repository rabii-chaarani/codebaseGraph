from __future__ import annotations

import json
import re
from pathlib import Path

from scripts import check_release_gate
from scripts.check_release_gate import (
    PYPI_CONFIRMATION_FLAGS,
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


def test_release_workflows_smoke_test_wheel_and_sdist() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")
        assert "pip install dist/*.whl" in text
        assert "pip install dist/*.tar.gz" in text


def test_release_workflow_enforces_production_gate_before_build() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "production-gate:" in text
    assert "python scripts/check_release_gate.py" in text
    assert "--production" in text
    assert "build:\n    name: build release distributions\n    needs:\n      - release-please\n      - production-gate" in text


def test_release_workflow_can_smoke_test_pypi_environment_without_publishing() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "pypi-environment-smoke:" in text
    assert "github.event_name == 'workflow_dispatch' && inputs.pypi-environment-smoke" in text
    assert "name: pypi" in text
    assert "id-token: write" in text
    assert "audience=pypi" in text
    assert '"environment": "pypi"' in text
    assert ".github/workflows/release.yml@" in text
    assert "pypi environment OIDC claims verified" in text


def test_release_please_is_skipped_during_pypi_environment_smoke() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert "release-please:\n    name: release please\n    if: ${{ !inputs.pypi-environment-smoke }}" in text


def test_release_please_uses_strict_semver_tags() -> None:
    config = json.loads(Path("release-please-config.json").read_text(encoding="utf-8"))
    root_package = config["packages"]["."]

    assert root_package["include-v-in-tag"] is True
    assert root_package["include-component-in-tag"] is False
    assert "release tag must match vX.Y.Z" in Path(".github/workflows/release.yml").read_text(encoding="utf-8")
    assert "component-prefixed tags" in Path("docs/release.md").read_text(encoding="utf-8")


def test_conda_recipe_uses_bounded_runtime_dependencies() -> None:
    text = Path("conda-forge/recipe/meta.yaml").read_text(encoding="utf-8")

    assert '{% set pypi_name = "cbasegraph" %}' in text
    assert "setuptools >=77" in text
    assert "real-ladybug >=0.15.3,<0.16" in text
    assert "tomli >=2.0.1" in text
    assert "tree-sitter >=0.25.2,<0.26" in text
    assert "tree-sitter-python >=0.25.0,<0.26" in text
    assert "license: MIT" in text
    assert "PUT_SPDX_LICENSE_ID_HERE" not in text


def test_hosted_workflows_run_real_vulnerability_scans() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")
        assert "pip_audit --strict" in text
        assert "pip_audit --strict ." in text
        assert "--skip-editable" not in text
        assert re.search(r"pip_audit\b[^\n]*--dry-run", text) is None


def test_supply_chain_workflow_audits_project_dependencies() -> None:
    text = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
    match = re.search(r"  supply-chain:\n(?P<body>.*?)(?=\n  [A-Za-z0-9_-]+:|\Z)", text, re.DOTALL)

    assert match is not None
    body = match.group("body")
    assert 'python -m pip install ".[dev]"' in body
    assert 'python -m pip install -e ".[dev]"' not in body
    assert "python -m pip_audit --strict ." in body


def test_project_metadata_uses_published_pypi_name() -> None:
    text = Path("pyproject.toml").read_text(encoding="utf-8")

    assert 'name = "cbasegraph"' in text


def test_security_policy_exists() -> None:
    text = Path("SECURITY.md").read_text(encoding="utf-8")

    assert "Reporting a Vulnerability" in text
    assert "graph_query" in text
    assert "--allow-remote" in text


def test_release_docs_list_production_confirmation_flags() -> None:
    text = Path("docs/release.md").read_text(encoding="utf-8")

    assert "PyPI project: `cbasegraph`" in text
    for flag in PYPI_CONFIRMATION_FLAGS:
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

        assert "actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd" in text
        assert "actions/setup-python@a309ff8b426b58ec0e2a45f0f869d46889d02405" in text
        assert "actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd" not in text
        assert "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5" not in text
        assert "actions/setup-python@a26af69be951a213d495a4c3e4e4022e16d87065" not in text


def test_workflows_avoid_node20_artifact_actions() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")

        assert "actions/upload-artifact@" not in text
        assert "actions/download-artifact@" not in text


def test_release_workflow_downloads_distributions_from_github_release() -> None:
    text = Path(".github/workflows/release.yml").read_text(encoding="utf-8")

    assert 'gh release download "$RELEASE_TAG" --repo "$GITHUB_REPOSITORY" --dir dist' in text
    assert "release {artifacts=} does not include a wheel" in text
    assert "release {artifacts=} does not include a source distribution" in text


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
    assert "conda recipe still contains PUT_RELEASE_SDIST_SHA256_HERE." in messages
    assert "conda recipe still contains PUT_SPDX_LICENSE_ID_HERE." not in messages


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
