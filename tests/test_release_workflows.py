from __future__ import annotations

import re
from pathlib import Path


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
        assert "pip_audit --strict --dry-run" not in text
