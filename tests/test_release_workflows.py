from __future__ import annotations

import re
from pathlib import Path

from scripts.check_release_gate import _jobs_missing_timeout, run_checks


WORKFLOWS = (
    Path(".github/workflows/ci.yml"),
    Path(".github/workflows/release.yml"),
)


def test_github_actions_are_pinned_to_immutable_commits() -> None:
    mutable_refs: list[str] = []
    uses_pattern = re.compile(r"^\s*uses:\s*(?P<action>[^@\s]+)@(?P<ref>[0-9a-f]{40}|[^\s#]+)")

    for path in WORKFLOWS:
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            match = uses_pattern.match(line)
            if match is None:
                continue
            if not re.fullmatch(r"[0-9a-f]{40}", match.group("ref")):
                mutable_refs.append(f"{path}:{line_number}: {match.group('action')}@{match.group('ref')}")

    assert mutable_refs == []


def test_release_workflows_smoke_test_wheel_and_sdist() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")
        assert "pip install dist/*.whl" in text
        assert "pip install dist/*.tar.gz" in text


def test_hosted_workflows_run_real_vulnerability_scans() -> None:
    for path in WORKFLOWS:
        text = path.read_text(encoding="utf-8")
        assert "pip_audit --strict" in text
        assert re.search(r"pip_audit\b[^\n]*--dry-run", text) is None


def test_security_policy_exists() -> None:
    text = Path("SECURITY.md").read_text(encoding="utf-8")

    assert "Reporting a Vulnerability" in text
    assert "graph_query" in text
    assert "--allow-remote" in text


def test_workflow_jobs_have_timeouts() -> None:
    for path in WORKFLOWS:
        missing = _jobs_missing_timeout(path.read_text(encoding="utf-8"))

        assert missing == []


def test_local_release_gate_passes() -> None:
    assert run_checks(production=False, require_conda=False, confirmations=set()) == []


def test_production_release_gate_reports_owner_controlled_blockers() -> None:
    issues = run_checks(production=True, require_conda=True, confirmations=set())
    codes = {issue.code for issue in issues}

    assert "license-metadata-missing" in codes
    assert "license-file-missing" in codes
    assert "external-confirmation-missing" in codes
    assert "conda-placeholder" in codes
