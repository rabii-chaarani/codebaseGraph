"""Audit archived executions without treating them as aggregate acceptance.

Run with Python 3.8+: python3 docs/testing/graph-disk-cleanup-evidence/audit.py
This checks evidence integrity, not implementation correctness or platform gates.
"""

import hashlib
import gzip
import json
from pathlib import Path
import xml.etree.ElementTree as ET


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def passed(case):
    return not any(case.find(tag) is not None for tag in ("failure", "error", "skipped"))


def audit(root=None):
    root = Path(root) if root is not None else Path(__file__).resolve().parent
    index = json.loads((root.parent / "graph-disk-cleanup-evidence.json").read_text())
    reports = json.loads((root / "foundation-reports.json").read_text())["reports"]
    report_ids = set()
    executions = 0
    cases_by_report = {}
    for report in reports:
        report_id = report["id"]
        assert report_id not in report_ids, report_id
        report_ids.add(report_id)
        path = root / report["report"]
        assert sha256(path) == report["sha256"], path
        xml = ET.parse(path).getroot()
        cases = xml.findall(".//testcase")
        cases_by_report[report_id] = cases
        actual = {
            "cases": len(cases),
            "failed": sum(bool(c.findall("failure") + c.findall("error")) for c in cases),
            "skipped": sum(c.find("skipped") is not None for c in cases),
        }
        assert actual == report["execution_counts"], (report_id, actual)
        names = [
            {"suite": suite.get("name"), "name": case.get("name"), "classname": case.get("classname")}
            for suite in xml.findall(".//testsuite")
            for case in suite.findall("testcase")
        ]
        assert names == report["case_names"], report_id
        if "source_manifest" in report:
            assert sha256(root / report["source_manifest"]) == report["source_manifest_sha256"]
        executions += actual["cases"]

    ids = [row["id"] for row in index["traceability"]]
    assert len(ids) == len(set(ids)), "duplicate work identity"
    expected = {
        "DC-S%d-%s%d" % (step, kind, number)
        for step, requirements, acceptance in [(1, 8, 4), (2, 8, 4), (3, 8, 5), (4, 8, 4), (5, 9, 4)]
        for kind, count in [("R", requirements), ("A", acceptance)]
        for number in range(1, count + 1)
    }
    assert set(ids) == expected, (expected - set(ids), set(ids) - expected)
    by_id = {report["id"]: report for report in reports}
    for row in index["traceability"]:
        for evidence in row["executed_evidence"]:
            report = by_id[evidence["report_id"]]
            names = {case["name"] for case in report["case_names"]}
            assert evidence["tests"] and set(evidence["tests"]) <= names, row["id"]
            if evidence["result"] == "passed":
                for name in evidence["tests"]:
                    referenced = [case for case in cases_by_report[evidence["report_id"]] if case.get("name") == name]
                    assert referenced and all(passed(case) for case in referenced), (row["id"], name)
    msrv = json.loads((root / "msrv-readiness.json").read_text())
    for attempt in msrv["attempts"]:
        assert sha256(root / attempt["stderr"]) == attempt["stderr_sha256"]
        assert attempt["actual_exit_code"] == 101
    assert msrv["baseline_manifest_lock_diff"] == "", "dependency baseline changed"
    reviews = json.loads((root / "review-outcomes.json").read_text())
    for artifact in reviews["s3_expanded_partial"]["executed_failure_artifacts"]:
        assert sha256(root / artifact["path"]) == artifact["sha256"]
        if artifact.get("encoding") == "gzip":
            decoded = gzip.decompress((root / artifact["path"]).read_bytes())
            assert hashlib.sha256(decoded).hexdigest() == artifact["decoded_sha256"]
        else:
            assert artifact.get("encoding") is None, "unsupported evidence encoding"
    scheduler = reviews["s4_scheduler_primitive"]["actual_junit"]
    assert sha256(root / scheduler["path"]) == scheduler["sha256"]
    external = json.loads((root.parent / index["s5_external_execution"]).read_text())
    assert sha256(root / external["junit"]) == external["junit_sha256"]
    external_xml = ET.parse(root / external["junit"]).getroot()
    external_cases = external_xml.findall(".//testcase")
    assert external_xml.get("uuid") == external["run_id"], "external run identity mismatch"
    actual_names = sorted((case.get("name"), case.get("classname")) for case in external_cases)
    expected_names = sorted((case["name"], case["classname"]) for case in external["expected_test_cases"])
    assert expected_names == sorted((name, "codebase-graph::storage_maintenance") for name in (
        "health_preserves_legacy_fields_and_marks_unobserved_maintenance_unknown",
        "nonboolean_quiescence_confirmation_is_rejected_before_repository_effects",
    )), "unexpected external test contract"
    assert actual_names == expected_names, "external case identity mismatch"
    actual_results = {
        "passed": sum(passed(case) for case in external_cases),
        "failed": sum(bool(case.findall("failure") + case.findall("error")) for case in external_cases),
        "skipped": sum(case.find("skipped") is not None for case in external_cases),
    }
    assert actual_results == external["actual_result"], "external result mismatch"
    assert actual_results == {"passed": 2, "failed": 0, "skipped": 0}
    assert "src/api/normalization.rs" in external["source_sha256"]
    for path, expected_hash in external["capture_files_sha256"].items():
        assert sha256(root / path) == expected_hash, path
    capture = root / "s5-fresh-capture"
    source_before = json.loads((capture / "source-before.json").read_text())
    source_after_build = json.loads((capture / "source-after-build.json").read_text())
    source_after_execution = json.loads((capture / "source-after-execution.json").read_text())
    assert source_before == source_after_build == source_after_execution == external["source_sha256"]
    binary_before = json.loads((capture / "binary-before.json").read_text())
    binary_after = json.loads((capture / "binary-after.json").read_text())
    assert binary_before == binary_after == external["binary_sha256"]
    print("Evidence integrity passed: %d traceability rows, %d reports, %d executions." % (len(ids), len(reports), executions))
    print("Execution counts are not unique scenarios. This audit does not establish aggregate acceptance.")
    print("Separate narrow S5 health/request report: 2 passed; final integration remains pending.")


if __name__ == "__main__":
    audit()
