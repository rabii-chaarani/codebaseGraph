"""Mutation regressions use isolated copies; original execution reports stay intact."""

import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("evidence_audit", ROOT / "audit.py")
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class EvidenceMutationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cgraph-s5-audit-regression-")
        self.addCleanup(self.temporary.cleanup)
        base = Path(self.temporary.name)
        self.root = base / ROOT.name
        shutil.copytree(ROOT, self.root, ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copy2(ROOT.parent / "graph-disk-cleanup-evidence.json", base)

    def read(self, name):
        return json.loads((self.root / name).read_text())

    def write(self, name, data):
        (self.root / name).write_text(json.dumps(data))

    def rewrite_report(self, record, mutate, path_key="junit", hash_key="junit_sha256"):
        path = self.root / record[path_key]
        tree = ET.parse(path)
        mutate(tree.getroot())
        tree.write(path, encoding="utf-8", xml_declaration=True)
        record[hash_key] = hashlib.sha256(path.read_bytes()).hexdigest()

    def external(self):
        index = json.loads((self.root.parent / "graph-disk-cleanup-evidence.json").read_text())
        return Path(index["s5_external_execution"]).name

    def rejected(self):
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(AssertionError):
            AUDIT.audit(self.root)

    def test_original_actual_reports_pass(self):
        with contextlib.redirect_stdout(io.StringIO()):
            AUDIT.audit(self.root)

    def test_named_foundation_skip_rejected_even_with_matching_hash_and_counts(self):
        data = self.read("foundation-reports.json")
        record = next(r for r in data["reports"] if r["id"] == "s1-post-main")
        def skip(xml):
            case = next(c for c in xml.findall(".//testcase") if "dc_s1_a1_" in c.get("name", ""))
            ET.SubElement(case, "skipped")
        self.rewrite_report(record, skip, "report", "sha256")
        record["execution_counts"]["skipped"] += 1
        self.write("foundation-reports.json", data)
        self.rejected()

    def test_external_skip_rejected_with_original_claimed_counts(self):
        name = self.external()
        data = self.read(name)
        self.rewrite_report(data, lambda xml: ET.SubElement(xml.findall(".//testcase")[0], "skipped"))
        self.write(name, data)
        self.rejected()

    def test_external_skip_rejected_even_with_matching_manifest_counts(self):
        name = self.external()
        data = self.read(name)
        self.rewrite_report(data, lambda xml: ET.SubElement(xml.findall(".//testcase")[0], "skipped"))
        data["actual_result"] = {"passed": 1, "failed": 0, "skipped": 1}
        self.write(name, data)
        self.rejected()

    def test_external_claimed_failure_count_mismatch_rejected(self):
        name = self.external()
        data = self.read(name)
        data["actual_result"]["failed"] = 1
        self.write(name, data)
        self.rejected()

    def test_external_case_name_mismatch_rejected_despite_valid_hash(self):
        name = self.external()
        data = self.read(name)
        self.rewrite_report(data, lambda xml: xml.findall(".//testcase")[0].set("name", "different_case"))
        self.write(name, data)
        self.rejected()

    def test_external_run_identity_mismatch_rejected_despite_valid_hash(self):
        name = self.external()
        data = self.read(name)
        self.rewrite_report(data, lambda xml: xml.set("uuid", "different-run"))
        self.write(name, data)
        self.rejected()

    def test_external_missing_case_rejected_despite_valid_hash(self):
        name = self.external()
        data = self.read(name)
        def remove(xml):
            suite = xml.find("testsuite")
            suite.remove(suite.find("testcase"))
        self.rewrite_report(data, remove)
        self.write(name, data)
        self.rejected()


if __name__ == "__main__":
    unittest.main()
