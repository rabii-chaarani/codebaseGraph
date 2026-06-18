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
RUST_ONLY_DOCS = (
    Path("README.md"),
    Path("docs/release.md"),
    Path("docs/rust_rewrite.md"),
)
SUPPORTED_ARTIFACT_SUFFIXES = ("linux-x86_64", "macos-universal", "windows-x86_64")
RELEASE_CONFIRMATION_FLAGS = (
    "release-environment",
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
        choices=RELEASE_CONFIRMATION_FLAGS,
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
    issues.extend(_check_rust_only_documentation())
    issues.extend(_check_workflows())
    issues.extend(_check_release_workflow_permissions())
    issues.extend(_check_no_python_package_metadata())
    issues.extend(_check_thin_python_entrypoints())
    issues.extend(_check_no_python_mcp_db_fallbacks())
    issues.extend(_check_no_retired_native_feature_gate())
    issues.extend(_check_no_python_package_artifacts())
    if production:
        issues.extend(_check_rust_license_metadata())
        issues.extend(_check_external_confirmations(confirmations))
    if require_conda:
        issues.extend(_check_conda_recipe())
    return issues


def _check_security_policy() -> list[GateIssue]:
    issues: list[GateIssue] = []
    security = REPO_ROOT / "SECURITY.md"
    if not security.exists():
        issues.append(GateIssue("FAIL", "security-policy-missing", "SECURITY.md is required."))
    if security.exists():
        text = security.read_text(encoding="utf-8")
        for required in ("Reporting a Vulnerability", "graph_query", "--allow-remote"):
            if required not in text:
                issues.append(GateIssue("FAIL", "security-policy-incomplete", f"SECURITY.md must mention {required!r}."))
    return issues


def _check_rust_only_documentation() -> list[GateIssue]:
    issues: list[GateIssue] = []
    forbidden_patterns = (
        ("codebase_graph_native=1", "rust-native-opt-in-env"),
        ("native opt-in", "rust-native-opt-in-language"),
        ("rust is experimental", "rust-experimental-language"),
        ("python keeps ownership", "python-production-ownership-language"),
        ("golden parity fixtures should compare legacy python and native output", "python-test-oracle-language"),
        ("until the python path is removed", "python-test-oracle-language"),
    )
    for relative_path in RUST_ONLY_DOCS:
        path = REPO_ROOT / relative_path
        if not path.exists():
            issues.append(GateIssue("FAIL", "rust-only-doc-missing", f"{relative_path} is required."))
            continue
        text = path.read_text(encoding="utf-8").lower()
        for pattern, code in forbidden_patterns:
            if pattern in text:
                issues.append(
                    GateIssue(
                        "FAIL",
                        code,
                        f"{relative_path} must not describe Rust production behavior as opt-in, experimental, or Python-owned.",
                    )
                )
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
        if "cargo build --manifest-path rust/Cargo.toml --locked --release --bin codebase-graph" not in text:
            issues.append(
                GateIssue("FAIL", "workflow-native-build-missing", f"{relative_path} must build the Rust production binary.")
            )
        if ".tar.gz" not in text:
            issues.append(
                GateIssue("FAIL", "workflow-native-archive-missing", f"{relative_path} must archive the Rust production binary.")
            )
        if relative_path.name == "release.yml":
            if "codebase-graph-${" not in text:
                issues.append(
                    GateIssue("FAIL", "workflow-versioned-native-archive-missing", f"{relative_path} must version the Rust archive.")
                )
            if ".tar.gz.sha256" not in text:
                issues.append(
                    GateIssue("FAIL", "workflow-native-checksum-missing", f"{relative_path} must publish per-archive checksums.")
                )
        for suffix in SUPPORTED_ARTIFACT_SUFFIXES:
            if suffix not in text:
                issues.append(
                    GateIssue(
                        "FAIL",
                        "workflow-native-platform-missing",
                        f"{relative_path} must build a native archive for {suffix}.",
                    )
                )
        if "pypa/gh-action-pypi-publish" in text:
            issues.append(GateIssue("FAIL", "workflow-pypi-publish-forbidden", f"{relative_path} must not publish to PyPI."))
        if "codebase-graph-mcp" in text:
            issues.append(
                GateIssue(
                    "FAIL",
                    "workflow-python-mcp-sidecar-forbidden",
                    f"{relative_path} must use `codebase-graph mcp`, not a legacy codebase-graph-mcp sidecar.",
                )
            )
        if "python -m maturin build --release" in text:
            issues.append(
                GateIssue("FAIL", "workflow-python-wheel-build-forbidden", f"{relative_path} must not build Python wheels as release artifacts.")
            )
        if "python -m build" in text:
            issues.append(
                GateIssue(
                    "FAIL",
                    "workflow-python-build-forbidden",
                    f"{relative_path} must not build production distributions with python -m build.",
                )
            )
        if "dist/smoke/${{ matrix.binary }}" not in text or "--help" not in text:
            issues.append(
                GateIssue("FAIL", "workflow-native-smoke-missing", f"{relative_path} must smoke-test the Rust production binary.")
            )
        for required in ("graph-schema --json", "setup --repo-root tests/fixtures/sample_project", "scripts/smoke_native_artifact.py"):
            if required not in text:
                issues.append(
                    GateIssue("FAIL", "workflow-native-smoke-incomplete", f"{relative_path} must smoke-test {required}.")
                )
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
    if "environment:" not in text or "name: release" not in text:
        issues.append(
            GateIssue("FAIL", "release-environment-missing", "release workflow must use the protected release environment.")
        )
    if "contents: write" not in text:
        issues.append(GateIssue("FAIL", "release-write-permission-missing", "release workflow must upload native artifacts."))
    return issues


def _check_rust_license_metadata() -> list[GateIssue]:
    cargo = _load_toml(REPO_ROOT / "rust/crates/codebase_graph_native/Cargo.toml")
    package = cargo.get("package", {})
    license_value = package.get("license")
    repository = package.get("repository")
    readme = package.get("readme")
    license_paths = [path for path in REPO_ROOT.iterdir() if path.name.upper().startswith("LICENSE")]
    issues: list[GateIssue] = []
    if not license_value:
        issues.append(GateIssue("FAIL", "license-metadata-missing", "Cargo.toml must declare Rust package license metadata."))
    if not repository:
        issues.append(GateIssue("FAIL", "repository-metadata-missing", "Cargo.toml must declare Rust package repository metadata."))
    if not readme:
        issues.append(GateIssue("FAIL", "readme-metadata-missing", "Cargo.toml must declare Rust package readme metadata."))
    if not license_paths:
        issues.append(GateIssue("FAIL", "license-file-missing", "repository must include the selected license file."))
    return issues


def _check_no_python_package_artifacts() -> list[GateIssue]:
    forbidden_patterns = (
        "src/**/*.egg-info",
        "src/**/*.so",
        "src/**/*.pyd",
        "src/**/*.dylib",
    )
    issues: list[GateIssue] = []
    for pattern in forbidden_patterns:
        for path in REPO_ROOT.glob(pattern):
            issues.append(
                GateIssue(
                    "FAIL",
                    "python-package-artifact-present",
                    f"generated Python package artifact must not be present in release state: {path.relative_to(REPO_ROOT)}.",
                )
            )
    return issues


def _check_no_python_package_metadata() -> list[GateIssue]:
    pyproject_path = REPO_ROOT / "pyproject.toml"
    if not pyproject_path.exists():
        return []
    text = pyproject_path.read_text(encoding="utf-8")
    payload = _load_toml(pyproject_path)
    issues: list[GateIssue] = []
    if "build-system" in payload:
        issues.append(
            GateIssue(
                "FAIL",
                "python-build-system-present",
                "pyproject.toml must not define [build-system]; production packaging is Rust-owned.",
            )
        )
    if "project" in payload:
        issues.append(
            GateIssue(
                "FAIL",
                "python-project-metadata-present",
                "pyproject.toml must not define [project]; Rust Cargo metadata owns production packaging.",
            )
        )
    if "[project.scripts]" in text or "console_scripts" in text:
        issues.append(
            GateIssue(
                "FAIL",
                "python-entrypoint-metadata-present",
                "pyproject.toml must not define Python production entrypoints.",
            )
        )
    if "maturin" in payload.get("tool", {}):
        issues.append(
            GateIssue(
                "FAIL",
                "python-maturin-metadata-present",
                "pyproject.toml must not define [tool.maturin]; PyO3 dev builds are not production packaging.",
            )
        )
    return issues


def _check_thin_python_entrypoints() -> list[GateIssue]:
    forbidden_tokens_by_path = {
        Path("src/codebase_graph/cli/__init__.py"): (
            "argparse",
            "GraphMaterializer",
            "SearchService",
            "run_setup",
            "codebase_graph._native.product_cli",
            "run_product_cli",
            "CODEBASE_GRAPH_FORCE_PYTHON_CLI",
        ),
        Path("src/codebase_graph/mcp/server.py"): (
            "argparse",
            "serve_stdio",
            "serve_http",
            "build_http_server",
            "codebase_graph._native.product_cli",
            "run_product_cli",
            "CODEBASE_GRAPH_FORCE_PYTHON_CLI",
        ),
    }
    issues: list[GateIssue] = []
    for relative_path, forbidden_tokens in forbidden_tokens_by_path.items():
        path = REPO_ROOT / relative_path
        if not path.exists():
            issues.append(
                GateIssue("FAIL", "python-entrypoint-missing", f"{relative_path} is required as a Rust native launcher.")
            )
            continue
        text = path.read_text(encoding="utf-8")
        for token in forbidden_tokens:
            if token in text:
                issues.append(
                    GateIssue(
                        "FAIL",
                        "python-entrypoint-fallback-present",
                        f"{relative_path} must stay a thin Rust native launcher; found {token}.",
                    )
                )
    return issues


def _check_no_python_mcp_db_fallbacks() -> list[GateIssue]:
    tools_path = REPO_ROOT / "src/codebase_graph/mcp/tools.py"
    if not tools_path.exists():
        return [
            GateIssue(
                "FAIL",
                "mcp-tools-missing",
                "src/codebase_graph/mcp/tools.py is required for MCP tool contract checks.",
            )
        ]
    text = tools_path.read_text(encoding="utf-8")
    issues: list[GateIssue] = []
    if "RUST_OWNED_TOOLS" not in text:
        issues.append(
            GateIssue(
                "FAIL",
                "mcp-rust-owned-routing-missing",
                "MCP DB-backed tools must be explicitly routed through the Rust native binary.",
            )
        )
    for token in ("def _native_tool_payload", "native_payload ="):
        if token in text:
            issues.append(
                GateIssue(
                    "FAIL",
                    "mcp-python-db-fallback-present",
                    f"MCP DB-backed tools must fail on Rust errors instead of using Python DB fallback code; found {token}.",
                )
            )
    return issues


def _check_no_retired_native_feature_gate() -> list[GateIssue]:
    issues: list[GateIssue] = []
    scanned_roots = (
        Path("src"),
        Path("scripts"),
        Path(".github"),
        Path("conda-forge"),
    )
    for root in scanned_roots:
        base = REPO_ROOT / root
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix in {".pyc", ".so", ".pyd", ".dylib"}:
                continue
            if path == Path(__file__).resolve():
                continue
            text = path.read_text(encoding="utf-8", errors="ignore")
            if re.search(r"\bCODEBASE_GRAPH_NATIVE\b", text):
                issues.append(
                    GateIssue(
                        "FAIL",
                        "retired-native-feature-gate-present",
                        f"{path.relative_to(REPO_ROOT)} must not use CODEBASE_GRAPH_NATIVE as a feature gate.",
                    )
                )
    return issues


def _check_external_confirmations(confirmations: set[str]) -> list[GateIssue]:
    return [
        GateIssue("FAIL", "external-confirmation-missing", f"production release requires --confirm {flag}.")
        for flag in RELEASE_CONFIRMATION_FLAGS
        if flag not in confirmations
    ]


def _check_conda_recipe() -> list[GateIssue]:
    recipe_path = REPO_ROOT / "conda-forge/recipe/meta.yaml"
    issues: list[GateIssue] = []
    if not recipe_path.exists():
        return [GateIssue("FAIL", "conda-recipe-missing", "conda-forge/recipe/meta.yaml is required.")]

    recipe = recipe_path.read_text(encoding="utf-8")
    for placeholder in ("PUT_RELEASE_VERSION_HERE", "PUT_RELEASE_ARCHIVE_SHA256_HERE", "PUT_SPDX_LICENSE_ID_HERE"):
        if placeholder in recipe:
            issues.append(GateIssue("FAIL", "conda-placeholder", f"conda recipe still contains {placeholder}."))
    if "codebase-graph-mcp" in recipe:
        issues.append(
            GateIssue(
                "FAIL",
                "conda-python-mcp-sidecar-forbidden",
                "conda recipe must not install or test the legacy codebase-graph-mcp sidecar.",
            )
        )
    return issues


def _load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


if __name__ == "__main__":
    raise SystemExit(main())
