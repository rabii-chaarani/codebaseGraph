"""Fresh, narrow DC-S5 execution capture; never promotes aggregate/platform gates.

Requires Python 3.9+. Run `python3 qualification.py capture --repository REPO --output NEW_DIRECTORY`
only while holding the coordinator's Cargo target lease. `verify DIRECTORY`
checks the archived capture without rebuilding or reading the original checkout.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import xml.etree.ElementTree as ET

SCHEMA = 2
BINARY = "codebase-graph::storage_maintenance"
EXPECTED = {
    "health_preserves_legacy_fields_and_marks_unobserved_maintenance_unknown",
    "nonboolean_quiescence_confirmation_is_rejected_before_repository_effects",
    "repeated_health_entries_preserve_source_and_foreign_sentinels",
}
LIMITATIONS = [
    "Narrow external storage_maintenance execution only; no collector qualification.",
    "Native Windows/Linux, MSRV, full suite and aggregate closure remain pending.",
    "Source identity covers repository build inputs; dependency provenance uses Cargo.lock and metadata.",
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    Path(path).write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")


def read_json(path):
    return json.loads(Path(path).read_text())


def source_manifest(repository):
    repository = Path(repository).resolve()
    # Include untracked new inputs and detect additions/deletions between phases.
    names = subprocess.check_output([
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
    ], cwd=repository).decode().split("\0")
    roots = {"src", "tests", "crates", "vendor", "assets", ".cargo"}
    singles = {"Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain", "rust-toolchain.toml"}
    result = {}
    for name in sorted(set(names) - {""}):
        if name.split("/")[0] not in roots and name not in singles:
            continue
        path = repository / name
        require(not path.is_symlink() and path.resolve().is_relative_to(repository),
                "build input is a symlink or escapes repository: " + name)
        result[name] = sha256(path)  # Missing tracked input fails closed.
    require({"Cargo.toml", "Cargo.lock", "build.rs"} <= result.keys(), "missing core build inputs")
    return result


def run(command, repository, output, label, env=None):
    with (output / (label + ".stdout")).open("wb") as stdout:
        with (output / (label + ".stderr")).open("wb") as stderr:
            result = subprocess.run(command, cwd=repository, stdout=stdout, stderr=stderr, check=False, env=env)
    return {"argv": command, "exit_code": result.returncode}


def binary_paths(metadata):
    binaries = metadata["rust-binaries"]
    require(set(binaries) == {BINARY}, "capture must select exactly the storage_maintenance binary")
    paths = {str(Path(row["binary-path"]).resolve()) for row in binaries.values()}
    # nextest can launch non-test helper executables too: freeze those as inputs.
    build = metadata["rust-build-meta"]
    for rows in build.get("non-test-binaries", {}).values():
        for row in rows:
            paths.add(str((Path(build["target-directory"]) / row["path"]).resolve()))
    return sorted(paths)


def junit_result(path):
    xml = ET.parse(path).getroot()
    cases = xml.findall(".//testcase")
    identities = [(case.get("classname"), case.get("name")) for case in cases]
    expected = {(BINARY, name) for name in EXPECTED}
    require(len(identities) == len(expected) and set(identities) == expected,
            "unexpected, missing or duplicate external testcase")
    require(all(not any(case.find(tag) is not None for tag in ("failure", "error", "skipped", "rerunFailure", "rerunError"))
                for case in cases), "external testcase did not pass on its first execution")
    return {"run_id": xml.get("uuid"), "passed": len(cases), "failed": 0, "skipped": 0}


def verify(output):
    output = Path(output)
    record = read_json(output / "capture.json")
    require(record["schema"] == SCHEMA, "unsupported capture schema")
    require(record["scope"] == "narrow_external_contract", "capture scope cannot claim qualification")
    require(record["qualification"] == {"aggregate": "pending", "windows": "pending", "linux": "pending"},
            "narrow capture cannot promote qualification gates")
    for name, digest in record["artifacts"].items():
        path = Path(name)
        require(not path.is_absolute() and ".." not in path.parts, "capture path escapes archive")
        require(not (output / path).is_symlink() and (output / path).resolve().is_relative_to(output.resolve()),
                "capture artifact escapes archive")
        require(sha256(output / path) == digest, "artifact changed: " + name)
    required = {"source-before.json", "source-after-build.json", "source-after-execution.json",
                "binary-before.json", "binary-after.json", "binary-list.stdout", "binary-list.stderr",
                "cargo-metadata.stdout", "cargo-metadata.stderr", "execution.stdout", "execution.stderr",
                "junit.xml", "nextest.toml", "capture-driver.py", "rustc.stdout", "rustc.stderr", "cargo.stdout", "cargo.stderr", "root-build.stdout", "root-build.stderr",
                "wrapper-build.stdout", "wrapper-build.stderr", "root-wrapper.rs", "root-wrapper.exe",
                "compile-proof.json", "build-environment.json", "root-rustc.stdout", "root-rustc.stderr"}
    require(set(record["artifacts"]) == required, "incomplete capture artifact set")
    sources = [read_json(output / name) for name in (
        "source-before.json", "source-after-build.json", "source-after-execution.json")]
    require(sources[0] and sources[0] == sources[1] == sources[2], "source changed during capture")
    require({"Cargo.toml", "Cargo.lock", "build.rs"} <= sources[0].keys(), "missing core build inputs")
    before, after = [read_json(output / name) for name in ("binary-before.json", "binary-after.json")]
    metadata = read_json(output / "binary-list.stdout")
    require(before and before == after and set(before) == set(binary_paths(metadata)), "binary identity mismatch")
    require(metadata["rust-build-meta"]["base-output-directories"] == ["debug"], "capture did not use debug build")
    commands = record["commands"]
    require(set(commands) == {"rustc", "cargo", "cargo-metadata", "wrapper-build", "root-build", "root-rustc", "binary-list", "execution"}, "missing command provenance")
    require(all(command["exit_code"] == 0 for command in commands.values()), "captured command failed")
    for label, argv in [("rustc", ["rustc", "-vV"]), ("cargo", ["cargo", "-V"]),
                        ("cargo-metadata", ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"])]:
        require(commands[label]["argv"] == argv, "unexpected " + label + " command")
    require(commands["binary-list"]["argv"] == ["cargo", "nextest", "list", "--offline", "--locked", "--test", "storage_maintenance", "--list-type", "binaries-only", "--message-format", "json"], "unexpected compile command")
    require(commands["execution"]["argv"] == execution_command(Path(record["output"])), "unexpected execution command")
    validate_compile_proof(record, output)
    result = junit_result(output / "junit.xml")
    require(result["run_id"] and result == record["result"], "JUnit result/run identity mismatch")
    require(record["limitations"] == LIMITATIONS, "qualification limitations changed")
    return record


def execution_command(output):
    return ["cargo", "nextest", "run", "--binaries-metadata", str(output / "binary-list.stdout"),
            "--cargo-metadata", str(output / "cargo-metadata.stdout"), "--config-file", str(output / "nextest.toml"),
            "--profile", "cleanup-s5-capture", "--test-threads", "1"]


def capture(repository, output):
    repository, output = Path(repository).resolve(), Path(output).resolve()
    require(not output.is_relative_to(repository), "capture output must be outside the repository")
    output.mkdir(parents=False, exist_ok=False)  # Never overwrite a historical execution.
    (output / "capture-driver.py").write_bytes(Path(__file__).read_bytes())
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository).decode().strip()
    write_json(output / "source-before.json", source_manifest(repository))
    # An absolute path avoids shared target directories overwriting other JUnit runs.
    (output / "nextest.toml").write_text('[profile.cleanup-s5-capture]\nfail-fast = false\nretries = 0\n'
        '[profile.cleanup-s5-capture.junit]\npath = ' + json.dumps(str(output / "junit.xml")) + '\n')
    commands = {}
    environment = build_environment(output)
    write_json(output / "build-environment.json", {name: environment[name] for name in BUILD_ENV})
    (output / "root-wrapper.rs").write_text(WRAPPER_SOURCE)
    (output / "rustc-events").mkdir()
    commands["wrapper-build"] = run(["rustc", "--edition=2021", str(output / "root-wrapper.rs"), "-o", str(output / "root-wrapper.exe")], repository, output, "wrapper-build", environment)
    require(commands["wrapper-build"]["exit_code"] == 0, "wrapper compilation failed")
    for label, argv in [
        ("rustc", ["rustc", "-vV"]), ("cargo", ["cargo", "-V"]),
        ("cargo-metadata", ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"]),
    ]:
        commands[label] = run(argv, repository, output, label, environment)
        require(commands[label]["exit_code"] == 0, label + " failed; raw output retained")
    commands["root-build"] = run(root_build_command(), repository, output, "root-build", environment)
    require(commands["root-build"]["exit_code"] == 0, "root build failed; raw output retained")
    for label, argv in [
        ("binary-list", ["cargo", "nextest", "list", "--offline", "--locked", "--test", "storage_maintenance", "--list-type", "binaries-only", "--message-format", "json"]),
    ]:
        commands[label] = run(argv, repository, output, label, environment)
        require(commands[label]["exit_code"] == 0, label + " failed; raw output retained")
    write_json(output / "source-after-build.json", source_manifest(repository))
    require(read_json(output / "source-before.json") == read_json(output / "source-after-build.json"), "source changed during build")
    paths = binary_paths(read_json(output / "binary-list.stdout"))
    proof = compile_proof(repository, output)
    write_json(output / "compile-proof.json", proof)
    compilers = {event["compiler"] for event in proof["events"]
                 if event["manifest"] == str(repository) and "--crate-name" in event["args"]}
    require(len(compilers) == 1, "root package used ambiguous compilers")
    commands["root-rustc"] = run([next(iter(compilers)), "-vV"], repository, output, "root-rustc", environment)
    require(commands["root-rustc"]["exit_code"] == 0, "actual root rustc identity failed")
    write_json(output / "binary-before.json", {path: sha256(path) for path in paths})
    commands["execution"] = run(execution_command(output), repository, output, "execution", environment)
    write_json(output / "binary-after.json", {path: sha256(path) for path in paths})
    write_json(output / "source-after-execution.json", source_manifest(repository))
    require(commands["execution"]["exit_code"] == 0, "execution failed; raw output retained")
    record = {
        "schema": SCHEMA, "scope": "narrow_external_contract", "head": head,
        "repository": str(repository), "output": str(output), "host": platform.platform(),
        "qualification": {"aggregate": "pending", "windows": "pending", "linux": "pending"},
        "commands": commands, "compile_proof": proof, "result": junit_result(output / "junit.xml"), "limitations": LIMITATIONS,
        "artifacts": {path.name: sha256(path) for path in sorted(output.iterdir()) if path.is_file()},
    }
    write_json(output / "capture.json", record)
    verify(output)
    return record


# A unique workspace-wrapper path is part of Cargo's crate hash. Each capture
# therefore recompiles workspace code, retaining non-workspace dependency caches.
# The wrapper records actual compiler execution and applies assertions LAST,
# after flags supplied by Cargo configuration or profiles.
WRAPPER_SOURCE = r'''
use std::{env, ffi::OsString, fs, process::{Command, exit}};
fn hex(value: &str) -> String {
    value.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    let mut args: Vec<OsString> = env::args_os().skip(1).collect();
    let compiler = args.remove(0);
    if args.iter().any(|arg| arg == "--crate-name") {
        args.push("-C".into());
        args.push("debug-assertions=on".into());
    }
    let mut evidence = format!("compiler={}\nmanifest={}\ncwd={}\n",
        hex(compiler.to_str().expect("non-UTF8 compiler path")),
        hex(&env::var("CARGO_MANIFEST_DIR").unwrap_or_default()),
        hex(env::current_dir().unwrap().to_str().expect("non-UTF8 cwd")));
    for arg in &args { evidence.push_str(&format!("arg={}\n", hex(arg.to_str().expect("non-UTF8 compiler arg")))); }
    let status = Command::new(&compiler).args(&args).status().expect("launch rustc");
    let code = status.code().unwrap_or(1);
    evidence.push_str(&format!("exit={code}\n"));
    let dir = env::var_os("CGRAPH_QUALIFICATION_EVENTS").expect("event directory");
    fs::write(std::path::PathBuf::from(dir).join(format!("{}.txt", std::process::id())), evidence).unwrap();
    exit(code);
}
'''
BUILD_ENV = ("RUSTC_WORKSPACE_WRAPPER", "CGRAPH_QUALIFICATION_EVENTS",
             "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS", "CARGO_PROFILE_DEV_DEBUG_ASSERTIONS")


def build_environment(output):
    environment = os.environ.copy()
    for name in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC"):
        require(not environment.get(name), "unsupported ambient compiler override: " + name)
    environment.update({
        "RUSTC_WORKSPACE_WRAPPER": str(output / "root-wrapper.exe"),
        "CGRAPH_QUALIFICATION_EVENTS": str(output / "rustc-events"),
        "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS": "true",
        "CARGO_PROFILE_DEV_DEBUG_ASSERTIONS": "true",
    })
    return environment


def root_build_command():
    return ["cargo", "test", "--offline", "--locked", "--package", "codebase-graph",
            "--test", "storage_maintenance", "--no-run", "--message-format=json", "-vv"]


def compile_proof(repository, output):
    events = []
    for path in sorted((output / "rustc-events").glob("*.txt")):
        event = {"args": []}
        for line in path.read_text().splitlines():
            key, value = line.split("=", 1)
            if key == "exit":
                event[key] = int(value)
            elif key == "arg":
                event["args"].append(bytes.fromhex(value).decode())
            else:
                event[key] = bytes.fromhex(value).decode()
        events.append(event)
    artifacts = [json.loads(line) for line in (output / "root-build.stdout").read_text().splitlines()
                 if line.startswith("{")]
    outputs = {}
    for artifact in artifacts:
        if artifact.get("reason") != "compiler-artifact" or "custom-build" in artifact["target"]["kind"]:
            continue
        name = artifact["target"]["name"].replace("-", "_")
        for event in events:
            args = event["args"]
            if event["manifest"] != str(repository) or "--crate-name" not in args or args[args.index("--crate-name") + 1] != name:
                continue
            actual_sources = [str(Path(event["cwd"]) / arg) if not Path(arg).is_absolute() else arg
                              for arg in args if arg.endswith(".rs")]
            if artifact["target"]["src_path"] not in actual_sources:
                continue
            for filename in artifact["filenames"]:
                emitted = compiler_output(event, artifact, filename)
                require(sha256(emitted) == sha256(filename), "Cargo artifact differs from actual compiler output")
                outputs[filename] = {"compiler_output": str(emitted), "sha256": sha256(emitted)}
    return {"events": events, "cargo_messages": artifacts, "outputs": outputs}


def compiler_output(event, artifact, filename):
    args = event["args"]
    out_dir = Path(args[args.index("--out-dir") + 1])
    path = Path(filename)
    if path.parent == out_dir:
        return path
    # Cargo copies normal bin targets from deps to the profile directory.
    require("bin" in artifact["target"]["kind"] and path.parent == out_dir.parent,
            "root output path mismatch")
    extra = [arg.split("=", 1)[1] for arg in args if arg.startswith("extra-filename=")]
    require(len(extra) == 1, "missing compiler output suffix")
    return out_dir / (artifact["target"]["name"].replace("-", "_") + extra[0] + (".exe" if path.suffix == ".exe" else ""))


def validate_compile_proof(record, output):
    repository = Path(record["repository"])
    original_output = Path(record["output"])
    require(record["commands"]["root-build"]["argv"] == root_build_command(), "unexpected root build command")
    require(record["commands"]["wrapper-build"]["argv"] == ["rustc", "--edition=2021", str(original_output / "root-wrapper.rs"), "-o", str(original_output / "root-wrapper.exe")], "unexpected wrapper build command")
    environment = read_json(output / "build-environment.json")
    require(environment == {
        "RUSTC_WORKSPACE_WRAPPER": str(original_output / "root-wrapper.exe"),
        "CGRAPH_QUALIFICATION_EVENTS": str(original_output / "rustc-events"),
        "CARGO_PROFILE_TEST_DEBUG_ASSERTIONS": "true", "CARGO_PROFILE_DEV_DEBUG_ASSERTIONS": "true",
    }, "unsupported build environment")
    require((output / "root-wrapper.rs").read_text() == WRAPPER_SOURCE, "compiler wrapper changed")
    metadata = read_json(output / "cargo-metadata.stdout")
    require(metadata["workspace_root"] == str(repository), "wrong Cargo workspace")
    packages = [p for p in metadata["packages"] if p["manifest_path"] == str(repository / "Cargo.toml")]
    require(len(packages) == 1 and packages[0]["name"] == "codebase-graph", "wrong root Cargo package")
    package = packages[0]
    require(package["id"] in metadata["workspace_members"], "root package is not a workspace member")
    proof = read_json(output / "compile-proof.json")
    require(proof == record["compile_proof"], "compile proof mismatch")
    raw_messages = [json.loads(line) for line in (output / "root-build.stdout").read_text().splitlines() if line.startswith("{")]
    require(proof["cargo_messages"] == raw_messages, "Cargo compile messages mismatch")
    artifacts = [message for message in raw_messages if message.get("reason") == "compiler-artifact"
                 and message["package_id"] == package["id"] and "custom-build" not in message["target"]["kind"]]
    require(artifacts, "missing root compile artifacts")
    selected = read_json(output / "binary-list.stdout")
    require(all(row["package-id"] == package["id"] for row in selected["rust-binaries"].values()), "nextest selected wrong Cargo package")
    require(set(selected["rust-build-meta"].get("non-test-binaries", {})) <= {package["id"]}, "wrong helper package")
    compilers = {event["compiler"] for event in proof["events"]
                 if event["manifest"] == str(repository) and "--crate-name" in event["args"]}
    require(len(compilers) == 1 and record["commands"]["root-rustc"]["argv"] == [next(iter(compilers)), "-vV"], "actual root compiler identity mismatch")
    compiled_paths = set()
    saw_library = False
    sources = read_json(output / "source-before.json")
    for artifact in artifacts:
        require(artifact["fresh"] is False, "root artifact reused from cache")
        require(artifact["profile"]["debug_assertions"] is True, "root profile disables debug assertions")
        source = Path(artifact["target"]["src_path"])
        require(source.is_relative_to(repository) and str(source.relative_to(repository)) in sources, "unrecorded root source")
        name = artifact["target"]["name"].replace("-", "_")
        matching = []
        for event in proof["events"]:
            args = event["args"]
            if event["manifest"] != str(repository) or event["exit"] != 0 or "--crate-name" not in args:
                continue
            if args[args.index("--crate-name") + 1] != name:
                continue
            actual_sources = [str(Path(event["cwd"]) / arg) if not Path(arg).is_absolute() else arg
                              for arg in args if arg.endswith(".rs")]
            if str(source) in actual_sources:
                matching.append(event)
        require(matching, "no actual rustc invocation for root artifact")
        for event in matching:
            require(event["args"][-2:] == ["-C", "debug-assertions=on"], "actual root compiler assertions are not enabled")
            for path in artifact["filenames"]:
                emitted = compiler_output(event, artifact, path)
                require(proof["outputs"][path]["compiler_output"] == str(emitted), "compiler output identity mismatch")
                if path in read_json(output / "binary-before.json"):
                    require(proof["outputs"][path]["sha256"] == read_json(output / "binary-before.json")[path], "selected binary differs from compiler output")
        compiled_paths.update(artifact["filenames"])
        saw_library |= "lib" in artifact["target"]["kind"]
    require(saw_library, "root library was not freshly compiled")
    require(set(binary_paths(selected)) <= compiled_paths, "selected binary/helper did not come from root compilation")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    command = commands.add_parser("capture")
    command.add_argument("--repository", required=True)
    command.add_argument("--output", required=True)
    command = commands.add_parser("verify")
    command.add_argument("output")
    args = parser.parse_args()
    record = capture(args.repository, args.output) if args.action == "capture" else verify(args.output)
    print("Verified %d narrow external cases; aggregate/platform gates remain pending." % record["result"]["passed"])


if __name__ == "__main__":
    main()
