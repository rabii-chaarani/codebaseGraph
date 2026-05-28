from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ImportError:  # pragma: no cover - Python 3.10 compatibility
    import tomli as tomllib


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = (
    Path(".github/workflows/ci.yml"),
    Path(".github/workflows/release.yml"),
)
PYPI_CONFIRMATION_FLAGS = (
    "trusted-publisher",
    "pypi-environment",
    "hosted-ci-green",
    "private-vulnerability-reporting",
)


@dataclass(frozen=True, slots=True)
class GateIssue:
    severity: str
    code: str
    message: str

    def line(self) -> str:
        return f"{self.severity}: {self.code}: {self.message}"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check local and production release readiness gates.")
    parser.add_argument("--production", action="store_true", help="Require owner-controlled production release gates.")
    parser.add_argument("--require-conda", action="store_true", help="Require the conda-forge recipe to be finalized.")
    parser.add_argument(
        "--confirm",
        action="append",
        default=[],
        choices=PYPI_CONFIRMATION_FLAGS,
        help="Confirm a manually verified external production gate.",
    )
    args = parser.parse_args(argv)

    issues = run_checks(
        production=args.production,
        require_conda=args.require_conda,
        confirmations=set(args.confirm),
    )
    if not issues:
        print("release gate passed")
        return 0
    for issue in issues:
        print(issue.line(), file=sys.stderr)
    return 1 if any(issue.severity == "FAIL" for issue in issues) else 0


def run_checks(*, production: bool, require_conda: bool, confirmations: set[str]) -> list[GateIssue]:
    issues: list[GateIssue] = []
    issues.extend(_check_security_policy())
    issues.extend(_check_workflows())
    issues.extend(_check_release_workflow_permissions())
    if production:
        issues.extend(_check_license_metadata())
        issues.extend(_check_external_confirmations(confirmations))
    if require_conda:
        issues.extend(_check_conda_recipe())
    return issues


def _check_security_policy() -> list[GateIssue]:
    issues: list[GateIssue] = []
    security = REPO_ROOT / "SECURITY.md"
    manifest = REPO_ROOT / "MANIFEST.in"
    if not security.exists():
        issues.append(GateIssue("FAIL", "security-policy-missing", "SECURITY.md is required."))
    if security.exists():
        text = security.read_text(encoding="utf-8")
        for required in ("Reporting a Vulnerability", "graph_query", "--allow-remote"):
            if required not in text:
                issues.append(GateIssue("FAIL", "security-policy-incomplete", f"SECURITY.md must mention {required!r}."))
    if not manifest.exists() or "include SECURITY.md" not in manifest.read_text(encoding="utf-8"):
        issues.append(GateIssue("FAIL", "security-policy-not-packaged", "MANIFEST.in must include SECURITY.md."))
    return issues


def _check_workflows() -> list[GateIssue]:
    issues: list[GateIssue] = []
    for relative_path in WORKFLOWS:
        path = REPO_ROOT / relative_path
        if not path.exists():
            issues.append(GateIssue("FAIL", "workflow-missing", f"{relative_path} is required."))
            continue
        text = path.read_text(encoding="utf-8")
        issues.extend(_workflow_action_pin_issues(relative_path, text))
        for job in _jobs_missing_timeout(text):
            issues.append(GateIssue("FAIL", "workflow-timeout-missing", f"{relative_path}:{job} has no timeout."))
        if re.search(r"pip_audit\b[^\n]*--dry-run", text):
            issues.append(GateIssue("FAIL", "workflow-audit-dry-run", f"{relative_path} uses pip-audit --dry-run."))
        if "pip install dist/*.whl" not in text:
            issues.append(GateIssue("FAIL", "workflow-wheel-smoke-missing", f"{relative_path} must smoke-test wheels."))
        if "pip install dist/*.tar.gz" not in text:
            issues.append(GateIssue("FAIL", "workflow-sdist-smoke-missing", f"{relative_path} must smoke-test sdists."))
    return issues


def _workflow_action_pin_issues(relative_path: Path, text: str) -> list[GateIssue]:
    issues: list[GateIssue] = []
    uses_pattern = re.compile(r"^\s*(?:-\s*)?uses:\s*(?P<target>[^\s#]+)")
    for line_number, line in enumerate(text.splitlines(), start=1):
        match = uses_pattern.match(line)
        if match is None:
            continue
        target = match.group("target").strip("'\"")
        if target.startswith(("./", "../")):
            continue
        if "@" not in target:
            issues.append(GateIssue("FAIL", "workflow-action-not-pinned", f"{relative_path}:{line_number}: {target}"))
            continue
        action, ref = target.rsplit("@", 1)
        if not re.fullmatch(r"[0-9a-fA-F]{40}", ref):
            issues.append(
                GateIssue("FAIL", "workflow-action-not-pinned", f"{relative_path}:{line_number}: {action}@{ref}")
            )
    return issues


def _jobs_missing_timeout(text: str) -> list[str]:
    missing: list[str] = []
    in_jobs = False
    current_job: str | None = None
    current_has_timeout = False

    for line in text.splitlines():
        if line == "jobs:":
            in_jobs = True
            continue
        if not in_jobs:
            continue
        if line and not line.startswith(" "):
            break
        job_match = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if job_match is not None:
            if current_job is not None and not current_has_timeout:
                missing.append(current_job)
            current_job = job_match.group(1)
            current_has_timeout = False
            continue
        if current_job is not None and re.match(r"^    timeout-minutes:\s*\d+\s*$", line):
            current_has_timeout = True

    if current_job is not None and not current_has_timeout:
        missing.append(current_job)
    return missing


def _check_release_workflow_permissions() -> list[GateIssue]:
    workflow = REPO_ROOT / ".github/workflows/release.yml"
    issues: list[GateIssue] = []
    if not workflow.exists():
        return [GateIssue("FAIL", "workflow-missing", ".github/workflows/release.yml is required.")]

    text = workflow.read_text(encoding="utf-8")
    if (
        "production-gate:" not in text
        or "python scripts/check_release_gate.py" not in text
        or "--production" not in text
        or "- production-gate" not in text
    ):
        issues.append(
            GateIssue(
                "FAIL",
                "release-production-gate-missing",
                "release workflow must run the production release gate before build/publish.",
            )
        )
    if "environment:" not in text or "name: pypi" not in text:
        issues.append(GateIssue("FAIL", "pypi-environment-missing", "release workflow must publish through pypi environment."))
    if "id-token: write" not in text:
        issues.append(GateIssue("FAIL", "pypi-oidc-missing", "release workflow must grant id-token: write."))
    if "pull-requests: write" not in text:
        issues.append(
            GateIssue(
                "FAIL",
                "release-pr-permission-missing",
                "release workflow must grant pull-requests: write for release-please PR creation.",
            )
        )
    if "skip-github-pull-request" in text:
        issues.append(
            GateIssue(
                "FAIL",
                "release-pr-creation-disabled",
                "release workflow must not disable release-please pull request creation.",
            )
        )
    return issues


def _check_license_metadata() -> list[GateIssue]:
    pyproject = _load_toml(REPO_ROOT / "pyproject.toml")
    project = pyproject.get("project", {})
    license_value = project.get("license")
    license_files = project.get("license-files") or pyproject.get("tool", {}).get("setuptools", {}).get("license-files")
    license_paths = [path for path in REPO_ROOT.iterdir() if path.name.upper().startswith("LICENSE")]
    issues: list[GateIssue] = []
    if not license_value and not license_files:
        issues.append(GateIssue("FAIL", "license-metadata-missing", "pyproject.toml must declare package license metadata."))
    if not license_paths:
        issues.append(GateIssue("FAIL", "license-file-missing", "repository must include the selected license file."))
    return issues


def _check_external_confirmations(confirmations: set[str]) -> list[GateIssue]:
    return [
        GateIssue("FAIL", "external-confirmation-missing", f"production release requires --confirm {flag}.")
        for flag in PYPI_CONFIRMATION_FLAGS
        if flag not in confirmations
    ]


def _check_conda_recipe() -> list[GateIssue]:
    recipe_path = REPO_ROOT / "conda-forge/recipe/meta.yaml"
    issues: list[GateIssue] = []
    if not recipe_path.exists():
        return [GateIssue("FAIL", "conda-recipe-missing", "conda-forge/recipe/meta.yaml is required.")]

    recipe = recipe_path.read_text(encoding="utf-8")
    for placeholder in ("PUT_RELEASE_VERSION_HERE", "PUT_RELEASE_SDIST_SHA256_HERE", "PUT_SPDX_LICENSE_ID_HERE"):
        if placeholder in recipe:
            issues.append(GateIssue("FAIL", "conda-placeholder", f"conda recipe still contains {placeholder}."))
    return issues


def _load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


if __name__ == "__main__":
    raise SystemExit(main())
