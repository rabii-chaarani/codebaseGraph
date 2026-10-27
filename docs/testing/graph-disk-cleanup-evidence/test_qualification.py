"""Protocol regressions. Synthetic archives here never qualify production behavior."""

import importlib.util
import json
import os
import subprocess
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("qualification", ROOT / "qualification.py")
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)


class QualificationProtocolTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cgraph-s5-qualification-protocol-")
        self.addCleanup(self.temporary.cleanup)
        self.output = Path(self.temporary.name)
        source = {name: "a" * 64 for name in ["Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs", "src/main.rs", "tests/storage_maintenance.rs"]}
        for name in ["source-before.json", "source-after-build.json", "source-after-execution.json"]:
            Q.write_json(self.output / name, source)
        binaries = {"/synthetic/target/debug/deps/storage_maintenance-test": "b" * 64,
                    "/synthetic/target/debug/codebase-graph": "c" * 64}
        for name in ["binary-before.json", "binary-after.json"]:
            Q.write_json(self.output / name, binaries)
        metadata = {"rust-build-meta": {
            "target-directory": "/synthetic/target", "base-output-directories": ["debug"],
            "non-test-binaries": {"synthetic-root": [{"path": "debug/codebase-graph"}]},
        }, "rust-binaries": {Q.BINARY: {"package-id": "synthetic-root", "binary-path": "/synthetic/target/debug/deps/storage_maintenance-test"}}}
        Q.write_json(self.output / "binary-list.stdout", metadata)
        self.xml = '<testsuites uuid="synthetic-protocol-run"><testsuite name="fixture">' + ''.join(
            '<testcase classname="%s" name="%s"/>' % (Q.BINARY, name) for name in sorted(Q.EXPECTED)
        ) + '</testsuite></testsuites>'
        (self.output / "junit.xml").write_text(self.xml)
        for name in ["binary-list.stderr", "cargo-metadata.stdout", "cargo-metadata.stderr",
                     "execution.stdout", "execution.stderr", "nextest.toml", "capture-driver.py", "rustc.stdout",
                     "rustc.stderr", "cargo.stdout", "cargo.stderr"]:
            (self.output / name).write_text("synthetic protocol fixture\n")
        repository = Path("/synthetic/repo")
        package = {"id": "synthetic-root", "name": "codebase-graph", "manifest_path": str(repository / "Cargo.toml")}
        Q.write_json(self.output / "cargo-metadata.stdout", {"workspace_root": str(repository), "workspace_members": [package["id"]], "packages": [package]})
        messages, events = [], []
        for name, kind, source, filename in [
            ("codebase_graph", "lib", "src/lib.rs", "/synthetic/target/debug/deps/libcodebase_graph.rlib"),
            ("codebase-graph", "bin", "src/main.rs", "/synthetic/target/debug/codebase-graph"),
            ("storage_maintenance", "test", "tests/storage_maintenance.rs", "/synthetic/target/debug/deps/storage_maintenance-test"),
        ]:
            messages.append({"reason": "compiler-artifact", "package_id": package["id"], "fresh": False,
                "profile": {"debug_assertions": True}, "target": {"name": name, "kind": [kind], "src_path": str(repository / source)}, "filenames": [filename]})
            events.append({"compiler": "/synthetic/rustc", "manifest": str(repository), "cwd": str(repository), "exit": 0,
                "args": ["--crate-name", name.replace("-", "_"), source, "--out-dir", str(Path(filename).parent), "-C", "debug-assertions=on"]})
        (self.output / "root-build.stdout").write_text("".join(json.dumps(m) + "\n" for m in messages))
        for name in ["root-build.stderr", "wrapper-build.stdout", "wrapper-build.stderr", "root-wrapper.exe", "root-rustc.stdout", "root-rustc.stderr"]:
            (self.output / name).write_text("synthetic protocol fixture\n")
        (self.output / "root-wrapper.rs").write_text(Q.WRAPPER_SOURCE)
        outputs = {m["filenames"][0]: {"compiler_output": m["filenames"][0], "sha256": binaries.get(m["filenames"][0], "d" * 64)} for m in messages}
        Q.write_json(self.output / "compile-proof.json", {"events": events, "cargo_messages": messages, "outputs": outputs})
        Q.write_json(self.output / "build-environment.json", {
            "RUSTC_WORKSPACE_WRAPPER": str(self.output / "root-wrapper.exe"), "CGRAPH_QUALIFICATION_EVENTS": str(self.output / "rustc-events"),
            "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS": "true", "CARGO_PROFILE_DEV_DEBUG_ASSERTIONS": "true"})
        Q.write_json(self.output / "inventory.stdout", {
            "rust-build-meta": metadata["rust-build-meta"], "test-count": len(Q.EXPECTED),
            "rust-suites": {Q.BINARY: {"binary-path": "/synthetic/target/debug/deps/storage_maintenance-test", "package-id": "synthetic-root",
                "testcases": {name: {"ignored": False, "filter-match": {"status": "matches"}} for name in Q.EXPECTED}}}})
        (self.output / "inventory.stderr").write_text("synthetic protocol inventory\n")
        self.record = {
            "schema": Q.SCHEMA, "scope": "storage_maintenance_suite", "output": str(self.output), "repository": str(repository),
            "compile_proof": {"events": events, "cargo_messages": messages, "outputs": outputs},
            "expected_case_names": sorted(Q.EXPECTED),
            "qualification": {"aggregate": "pending", "windows": "pending", "linux": "pending"},
            "limitations": Q.LIMITATIONS,
            "commands": {label: {"argv": argv, "exit_code": 0} for label, argv in [
                ("rustc", ["rustc", "-vV"]), ("cargo", ["cargo", "-V"]),
                ("cargo-metadata", ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"]),
                ("binary-list", ["cargo", "nextest", "list", "--offline", "--locked", "--test", "storage_maintenance", "--list-type", "binaries-only", "--message-format", "json"]),
                ("wrapper-build", ["rustc", "--edition=2021", str(self.output / "root-wrapper.rs"), "-o", str(self.output / "root-wrapper.exe")]),
                ("root-build", Q.root_build_command()),
                ("root-rustc", ["/synthetic/rustc", "-vV"]),
                ("inventory", Q.inventory_command(self.output)),
                ("execution", Q.execution_command(self.output)),
            ]},
            "result": Q.junit_result(self.output / "junit.xml"),
        }
        self.seal()

    def seal(self):
        self.record["artifacts"] = {p.name: Q.sha256(p) for p in self.output.iterdir() if p.name != "capture.json"}
        Q.write_json(self.output / "capture.json", self.record)

    def rejected(self, reason):
        with self.assertRaisesRegex(ValueError, reason):
            Q.verify(self.output)

    def test_protocol_fixture_is_valid_without_production_qualification(self):
        self.assertEqual(Q.verify(self.output)["qualification"]["aggregate"], "pending")

    def test_missing_and_changed_artifacts_rejected(self):
        (self.output / "execution.stderr").write_text("changed")
        self.rejected("artifact changed")

    def test_path_traversal_rejected(self):
        self.record["artifacts"]["../outside"] = "a" * 64
        Q.write_json(self.output / "capture.json", self.record)
        self.rejected("escapes archive")

    def test_source_change_even_with_rehashed_artifact_rejected(self):
        Q.write_json(self.output / "source-after-build.json", {"Cargo.toml": "d" * 64})
        self.seal()
        self.rejected("source changed")

    def test_source_addition_and_deletion_invalidate_snapshot(self):
        for name in ["Cargo.toml", "Cargo.lock", "build.rs"]:
            (self.output / name).write_text("fixture")
        with patch.object(Q.subprocess, "check_output", return_value=b"Cargo.toml\0Cargo.lock\0build.rs\0"):
            original = Q.source_manifest(self.output)
        (self.output / "new.rs").write_text("new")
        with patch.object(Q.subprocess, "check_output", return_value=b"Cargo.toml\0Cargo.lock\0build.rs\0src/new.rs\0"):
            (self.output / "src").mkdir()
            (self.output / "src/new.rs").write_text("new")
            self.assertNotEqual(Q.source_manifest(self.output), original)
        (self.output / "Cargo.lock").unlink()
        with patch.object(Q.subprocess, "check_output", return_value=b"Cargo.toml\0Cargo.lock\0build.rs\0"):
            with self.assertRaises(FileNotFoundError):
                Q.source_manifest(self.output)

    def test_changed_binary_rejected_even_with_rehashed_artifact(self):
        Q.write_json(self.output / "binary-after.json", {"/synthetic/other": "d" * 64})
        self.seal()
        self.rejected("binary identity")

    def test_unrecorded_helper_binary_rejected(self):
        for name in ["binary-before.json", "binary-after.json"]:
            data = Q.read_json(self.output / name)
            del data["/synthetic/target/debug/codebase-graph"]
            Q.write_json(self.output / name, data)
        self.seal()
        self.rejected("binary identity")

    def test_release_build_cannot_qualify_debug_capture(self):
        metadata = Q.read_json(self.output / "binary-list.stdout")
        metadata["rust-build-meta"]["base-output-directories"] = ["release"]
        Q.write_json(self.output / "binary-list.stdout", metadata)
        self.seal()
        self.rejected("debug build")

    def test_failed_build_or_execution_rejected(self):
        for label in ["binary-list", "execution"]:
            self.record["commands"][label]["exit_code"] = 101
            self.seal()
            self.rejected("command failed")
            self.record["commands"][label]["exit_code"] = 0

    def test_filtered_or_rebuilt_execution_rejected(self):
        self.record["commands"]["execution"]["argv"] += ["-E", "test(nothing)"]
        self.seal()
        self.rejected("execution command")

    def test_skip_failure_error_and_retry_are_not_passes(self):
        for tag in ["skipped", "failure", "error", "rerunFailure", "rerunError"]:
            (self.output / "junit.xml").write_text(self.xml.replace('/>', '><%s/></testcase>' % tag, 1))
            self.seal()
            self.rejected("first execution")

    def test_missing_extra_and_duplicate_cases_rejected(self):
        for change in [self.xml.replace('name="health_preserves', 'name="different_health_preserves'),
                       self.xml.replace('</testsuite>', '<testcase classname="extra" name="extra"/></testsuite>'),
                       self.xml.replace('</testsuite>', '<testcase classname="%s" name="%s"/></testsuite>' % (Q.BINARY, sorted(Q.EXPECTED)[0]))]:
            (self.output / "junit.xml").write_text(change)
            self.seal()
            self.rejected("testcase")

    def test_result_and_run_identity_mismatch_rejected(self):
        self.record["result"]["run_id"] = "another-run"
        self.seal()
        self.rejected("identity mismatch")

    def test_platform_or_aggregate_promotion_rejected(self):
        for gate in ["aggregate", "windows", "linux"]:
            self.record["qualification"][gate] = "passed"
            self.seal()
            self.rejected("qualification gates")
            self.record["qualification"][gate] = "pending"

    def test_capture_refuses_existing_output_before_any_build(self):
        with self.assertRaises(FileExistsError):
            Q.capture(ROOT, self.output)

    def mutate_proof(self, mutate):
        proof = Q.read_json(self.output / "compile-proof.json")
        mutate(proof)
        Q.write_json(self.output / "compile-proof.json", proof)
        self.record["compile_proof"] = proof
        (self.output / "root-build.stdout").write_text("".join(json.dumps(m) + "\n" for m in proof["cargo_messages"]))
        self.seal()

    def test_cached_root_artifact_is_rejected(self):
        self.mutate_proof(lambda proof: proof["cargo_messages"][0].update(fresh=True))
        self.rejected("reused from cache")

    def test_assertion_disabled_root_profile_is_rejected(self):
        self.mutate_proof(lambda proof: proof["cargo_messages"][0]["profile"].update(debug_assertions=False))
        self.rejected("profile disables")

    def test_missing_actual_root_compiler_invocation_is_rejected(self):
        self.mutate_proof(lambda proof: proof.update(events=[]))
        self.rejected("actual root compiler identity|actual rustc invocation")

    def test_assertion_disabled_actual_compiler_is_rejected(self):
        self.mutate_proof(lambda proof: proof["events"][0]["args"].__setitem__(-1, "debug-assertions=off"))
        self.rejected("compiler assertions")

    def test_wrong_workspace_and_package_are_rejected(self):
        data = Q.read_json(self.output / "cargo-metadata.stdout")
        data["workspace_root"] = "/another/repository"
        Q.write_json(self.output / "cargo-metadata.stdout", data)
        self.seal()
        self.rejected("wrong Cargo workspace")

    def test_ambient_rustflags_are_rejected_before_compilation(self):
        with patch.dict(os.environ, {"RUSTFLAGS": "-C debug-assertions=off"}):
            with self.assertRaisesRegex(ValueError, "unsupported ambient compiler override"):
                Q.build_environment(self.output)

    def test_current_suite_inventory_cannot_hide_filtered_or_ignored_cases(self):
        original = Q.read_json(self.output / "inventory.stdout")
        for field in ["ignored", "filter-match"]:
            inventory = json.loads(json.dumps(original))
            case = next(iter(inventory["rust-suites"][Q.BINARY]["testcases"].values()))
            case[field] = True if field == "ignored" else {"status": "mismatch"}
            Q.write_json(self.output / "inventory.stdout", inventory)
            self.seal()
            self.rejected("ignored or filtered")

    def test_added_registered_case_must_execute_in_junit(self):
        inventory = Q.read_json(self.output / "inventory.stdout")
        inventory["test-count"] += 1
        inventory["rust-suites"][Q.BINARY]["testcases"]["new_real_case"] = {"ignored": False, "filter-match": {"status": "matches"}}
        Q.write_json(self.output / "inventory.stdout", inventory)
        self.record["expected_case_names"].append("new_real_case")
        self.record["expected_case_names"].sort()
        self.seal()
        self.rejected("testcase")

    def test_source_workload_cannot_pass_only_the_three_historical_cases(self):
        for name in ["source-before.json", "source-after-build.json", "source-after-execution.json"]:
            source = Q.read_json(self.output / name)
            source["tests/support/storage_maintenance/workload.rs"] = "e" * 64
            Q.write_json(self.output / name, source)
        self.seal()
        self.rejected("required fixture test cases")

    def test_inventory_binary_package_and_named_identity_must_match(self):
        inventory = Q.read_json(self.output / "inventory.stdout")
        inventory["rust-suites"][Q.BINARY]["package-id"] = "foreign"
        Q.write_json(self.output / "inventory.stdout", inventory)
        self.seal()
        self.rejected("binary/package identity")

    def test_source_bound_eight_and_nine_case_inventories_are_accepted_exactly(self):
        names = set(Q.EXPECTED) | Q.WORKLOAD_CASES
        for include_process in [False, True]:
            if include_process:
                names.add(Q.PROCESS_CASE)
            for name in ["source-before.json", "source-after-build.json", "source-after-execution.json"]:
                source = Q.read_json(self.output / name)
                source["tests/support/storage_maintenance/workload.rs"] = "e" * 64
                if include_process:
                    source["tests/support/storage_maintenance/process.rs"] = "f" * 64
                Q.write_json(self.output / name, source)
            inventory = Q.read_json(self.output / "inventory.stdout")
            inventory["test-count"] = len(names)
            inventory["rust-suites"][Q.BINARY]["testcases"] = {name: {"ignored": False, "filter-match": {"status": "matches"}} for name in names}
            Q.write_json(self.output / "inventory.stdout", inventory)
            xml = '<testsuites uuid="synthetic-expanded-suite"><testsuite>' + ''.join('<testcase classname="%s" name="%s"/>' % (Q.BINARY, name) for name in sorted(names)) + '</testsuite></testsuites>'
            (self.output / "junit.xml").write_text(xml)
            self.record["expected_case_names"] = sorted(names)
            self.record["result"] = Q.junit_result(self.output / "junit.xml", names)
            self.seal()
            self.assertEqual(Q.verify(self.output)["result"]["passed"], 9 if include_process else 8)

    def test_historical_schema_two_keeps_its_exact_three_case_scope(self):
        self.record["schema"] = 2
        self.record["scope"] = "narrow_external_contract"
        self.record["limitations"] = Q.LEGACY_LIMITATIONS
        del self.record["commands"]["inventory"]
        for name in ["inventory.stdout", "inventory.stderr"]:
            (self.output / name).unlink()
        self.seal()
        historical = Q.verify(self.output)
        self.assertEqual(historical["result"]["passed"], 3)
        self.assertEqual(historical["schema"], 2)
        self.assertEqual(historical["scope"], "narrow_external_contract")

    def test_real_subprocess_exit_and_output_are_captured(self):
        result = Q.run([sys.executable, "-c", "import sys; print('actual output'); sys.stderr.write('actual error'); sys.exit(7)"], self.output, self.output, "probe")
        self.assertEqual(result["exit_code"], 7)
        self.assertEqual((self.output / "probe.stdout").read_text(), "actual output\n")
        self.assertEqual((self.output / "probe.stderr").read_text(), "actual error")


class QualificationRuntimeTests(unittest.TestCase):
    """Real Cargo/nextest counterexamples, using only a tiny fixture-owned target.

    These run normally with this module. --protocol-only is available while the
    coordinator's Cargo lease belongs to another slice; it is not qualification.
    """
    def setUp(self):
        evidence_root = os.environ.get("CGRAPH_QUALIFICATION_RUNTIME_OUTPUT")
        if evidence_root:
            self.base = Path(tempfile.mkdtemp(prefix=self._testMethodName + "-", dir=evidence_root)).resolve()
            print("Real provenance regression archive: " + str(self.base), file=sys.stderr)
        else:
            self.temporary = tempfile.TemporaryDirectory(prefix="cgraph-s5-real-provenance-")
            self.addCleanup(self.temporary.cleanup)
            self.base = Path(self.temporary.name).resolve()
        self.repository = self.base / "repo"
        self.repository.mkdir()
        (self.repository / "src").mkdir()
        (self.repository / "tests").mkdir()
        (self.repository / "Cargo.toml").write_text('[package]\nname="codebase-graph"\nversion="0.0.0"\nedition="2021"\n')
        (self.repository / "build.rs").write_text("fn main() {}\n")
        (self.repository / "src/lib.rs").write_text("pub fn assertions_enabled() -> bool { cfg!(debug_assertions) }\n")
        (self.repository / "src/main.rs").write_text("fn main() { assert!(cfg!(debug_assertions)); }\n")
        self.test_source = self.repository / "tests/storage_maintenance.rs"
        self.write_assertions("assert!(true)")
        environment = os.environ.copy()
        environment["CARGO_TARGET_DIR"] = str(self.base / "target")
        for name in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"]:
            environment.pop(name, None)
        self.environment = environment
        for command in [
            ["git", "init", "--quiet"], ["cargo", "generate-lockfile", "--offline"],
            ["git", "add", "."], ["git", "-c", "user.name=Qualification fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "test: isolated fixture"],
        ]:
            subprocess.run(command, cwd=self.repository, env=environment, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    def write_assertions(self, assertion):
        self.test_source.write_text("".join(
            '#[test]\nfn %s() { %s; assert!(codebase_graph::assertions_enabled()); assert!(std::process::Command::new(env!("CARGO_BIN_EXE_codebase-graph")).status().unwrap().success()); }\n' % (name, assertion)
            for name in sorted(Q.EXPECTED)))

    def capture(self, name):
        with patch.dict(os.environ, self.environment, clear=True):
            return Q.capture(self.repository, self.base / name)

    def test_mtime_preserved_failing_source_is_recompiled_and_fails(self):
        original = self.capture("before")
        old_stat = self.test_source.stat()
        self.write_assertions("assert!(false)")
        os.utime(self.test_source, ns=(old_stat.st_atime_ns, old_stat.st_mtime_ns))
        failing_hash = Q.sha256(self.test_source)
        with self.assertRaisesRegex(ValueError, "execution failed"):
            self.capture("after")
        self.assertEqual(Q.sha256(self.test_source), failing_hash)
        self.assertEqual(self.test_source.stat().st_mtime_ns, old_stat.st_mtime_ns)
        proof = Q.read_json(self.base / "after/compile-proof.json")
        root = [m for m in proof["cargo_messages"] if m.get("reason") == "compiler-artifact" and "custom-build" not in m["target"]["kind"]]
        self.assertTrue(root and all(m["fresh"] is False for m in root))
        self.assertNotEqual(Q.read_json(self.base / "before/source-before.json"), Q.read_json(self.base / "after/source-before.json"))
        self.assertEqual(original["result"]["passed"], 3)
        Q.write_json(self.base / "runtime-result.json", {
            "scenario": "mtime-preserved changed assertion", "baseline_result": original["result"],
            "failing_source_sha256": failing_hash, "mtime_before_ns": old_stat.st_mtime_ns,
            "mtime_after_ns": self.test_source.stat().st_mtime_ns,
            "changed_source_was_recompiled": all(m["fresh"] is False for m in root),
            "changed_source_capture_rejected": True,
            "actual_failed_junit_sha256": Q.sha256(self.base / "after/junit.xml"),
        })

    def test_assertion_disabled_environment_and_config_cannot_pass_disabled_tests(self):
        self.environment["CARGO_PROFILE_TEST_DEBUG_ASSERTIONS"] = "false"
        self.environment["CARGO_PROFILE_DEV_DEBUG_ASSERTIONS"] = "false"
        config = self.repository / ".cargo"
        config.mkdir()
        (config / "config.toml").write_text('[build]\nrustflags = ["-C", "debug-assertions=off"]\n')
        # Enabled-sensitive library, helper and all tests pass despite ambient off.
        enabled = self.capture("enabled")
        self.assertEqual(enabled["result"]["passed"], 3)
        self.write_assertions("assert!(!cfg!(debug_assertions))")
        with self.assertRaisesRegex(ValueError, "execution failed"):
            self.capture("disabled-counterexample")
        proof = Q.read_json(self.base / "enabled/compile-proof.json")
        self.assertTrue(any("debug-assertions=off" in e["args"] for e in proof["events"]))
        root_events = [e for e in proof["events"] if "--crate-name" in e["args"]]
        self.assertTrue(root_events and all(e["args"][-2:] == ["-C", "debug-assertions=on"] for e in root_events))
        Q.write_json(self.base / "runtime-result.json", {
            "scenario": "assertion-disabled environment and Cargo config", "enabled_result": enabled["result"],
            "ambient_test_debug_assertions": "false", "ambient_dev_debug_assertions": "false",
            "config_rustflags": ["-C", "debug-assertions=off"], "actual_root_compilers_end_assertions_on": True,
            "assertion_disabled_test_capture_rejected": True,
            "actual_failed_junit_sha256": Q.sha256(self.base / "disabled-counterexample/junit.xml"),
        })


if __name__ == "__main__":
    if "--protocol-only" in sys.argv:
        sys.argv.remove("--protocol-only")
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(QualificationProtocolTests)
        sys.exit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
    unittest.main()
