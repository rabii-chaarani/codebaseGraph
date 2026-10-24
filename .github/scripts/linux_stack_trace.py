#!/usr/bin/env python3
"""Failure-only, no-build backtrace of an existing Linux library test binary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import re
import shutil
import signal
import subprocess
import time


DEFAULT_CASE = "adapters::cli::tests::dispatch_materialize::git_diff_plan_scopes_to_changed_paths"
CAPTURE = r'''
import gdb, json, re
record = {"threads": [], "stop": gdb.execute("info program", to_string=True)}
observed = re.search(r"signal (SIG[A-Z0-9]+)", record["stop"])
record["signal_observed"] = observed.group(1) if observed else None
selected = gdb.selected_thread()
record["selected_thread"] = selected.num if selected else None
for thread in gdb.selected_inferior().threads():
    item = {"number": thread.num, "name": thread.name, "ptid": list(thread.ptid), "frames": []}
    try:
        thread.switch()
        frame = gdb.newest_frame()
        while frame is not None and len(item["frames"]) < 128:
            entry = {"level": frame.level(), "function": frame.name()}
            try: entry["pc"] = int(frame.pc())
            except Exception as error: entry["pc_error"] = str(error)
            for register in ("sp", "rsp"):
                try:
                    entry["sp"] = int(frame.read_register(register))
                    break
                except Exception: pass
            try:
                location = frame.find_sal()
                entry["file"] = location.symtab.fullname() if location.symtab else None
                entry["line"] = location.line
            except Exception as error: entry["source_error"] = str(error)
            item["frames"].append(entry)
            frame = frame.older()
    except Exception as error: item["unwind_error"] = str(error)
    values = [entry["sp"] for entry in item["frames"] if "sp" in entry]
    item["unwound_sp_span_bytes"] = max(values) - min(values) if values else None
    record["threads"].append(item)
with open(output_path, "w") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
'''


class FailedTestSelectionError(ValueError):
    pass


def select_failed_case(log):
    """Use actual harness records, never the last merely mentioned test path."""
    log = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", log)
    rust_path = r"[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*"
    record = re.compile(r"^[ \t]*test (" + rust_path + r") \.\.\.[ \t]*(.*)$")
    status = re.compile(r"^(ok|FAILED|ignored)(?:\b|,)")
    opened = []
    failed = []
    for line in log.splitlines():
        match = record.match(line)
        if match:
            name, tail = match.groups()
            result = status.match(tail)
            if result:
                if result.group(1) == "FAILED":
                    failed.append(name)
            else:
                opened.append(name)
        elif line.strip() in ("ok", "FAILED", "ignored") and len(opened) == 1:
            # --nocapture output can split a harness prefix from its result.
            name = opened.pop()
            if line.strip() == "FAILED":
                failed.append(name)
    aborted = re.search(r"overflowed its stack|fatal runtime error:.*stack overflow|"
                        r"signal:[ \t]*[0-9]+|SIGABRT|SIGSEGV|process abort signal", log)
    if aborted:
        candidates = list(dict.fromkeys(opened))
        if len(candidates) != 1:
            raise FailedTestSelectionError(
                "Abort has no unique incomplete actual test record: " + repr(candidates))
        return {"case": opened[-1], "selection": "last_incomplete_actual_record_at_abort"}
    if failed:
        return {"case": failed[-1], "selection": "last_actual_FAILED_record"}
    if "test result: ok." in log:
        return None
    raise FailedTestSelectionError("Failure log has no attributable actual test record")


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case", help="Explicit exact test name/fallback, verified against existing binary inventory")
    parser.add_argument("--failed-log", type=Path, help="Original combined Cargo/xtask output; select its actual failed test")
    parser.add_argument("--timeout-seconds", type=int, default=180)
    args = parser.parse_args()
    if args.case is not None and not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*", args.case):
        parser.error("case must be an exact Rust test path")
    if not 1 <= args.timeout_seconds <= 300:
        parser.error("timeout must be between 1 and 300 seconds")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    summary = {"platform": platform.platform(), "case": args.case,
               "source_sha": os.environ.get("SOURCE_SHA"), "no_build": True,
               "stack_rlimit_unchanged": list(resource.getrlimit(resource.RLIMIT_STACK))}
    if args.failed_log is not None:
        summary["failed_log"] = str(args.failed_log)
        try:
            selected_case = select_failed_case(args.failed_log.read_text(errors="replace"))
        except OSError as error:
            if args.case is None:
                summary["unavailable"] = "Failure log unavailable: " + str(error)
                write_json(output / "summary.json", summary)
                return
            selected_case = {"case": args.case, "selection": "explicit_fallback_log_unavailable"}
        except FailedTestSelectionError as error:
            # Ambiguity is never replaced by a stale default or a guessed case.
            summary["unavailable"] = str(error)
            write_json(output / "summary.json", summary)
            return
        if selected_case is None:
            summary["unavailable"] = "Original log records normal completion and no failed test"
            write_json(output / "summary.json", summary)
            return
        args.case = selected_case["case"]
        summary.update(selected_case)
    else:
        args.case = args.case or DEFAULT_CASE
        summary.update({"case": args.case, "selection": "explicit_or_default_exact_case"})
    write_json(output / "summary.json", summary)
    if platform.system() != "Linux" or not shutil.which("gdb"):
        summary["unavailable"] = "Linux and an existing GDB are required"
        write_json(output / "summary.json", summary)
        return
    # Match the repository's Cargo test-thread setting. Do not set stack-size
    # environment variables, change profiles, or rebuild any test/helper/SDK.
    env = os.environ.copy()
    env["RUST_TEST_THREADS"] = "1"
    summary["test_environment"] = {key: env.get(key) for key in ("RUST_TEST_THREADS", "RUST_MIN_STACK")}
    # Cargo applies this repository's isolated test-home configuration to the
    # original binary. Replay it only in the GDB debuggee; the diagnostic helper
    # retains its own environment and never reads/uploads credential settings.
    test_home = None
    try:
        import tomllib
        config = tomllib.loads(Path(".cargo/config.toml").read_text())
        entry = config.get("env", {}).get("HOME")
        if isinstance(entry, dict) and entry.get("force") and isinstance(entry.get("value"), str):
            test_home = entry["value"]
            if entry.get("relative"):
                test_home = str((Path.cwd() / test_home).resolve())
            if "\n" in test_home or "\r" in test_home:
                raise ValueError("multiline test-home configuration is unsupported")
    except ImportError:
        summary["test_home_replay_unavailable"] = "tomllib unavailable"
    summary["cargo_test_home_replayed"] = test_home is not None
    inventory = []
    selected = None
    for candidate in sorted(Path("target/debug/deps").glob("codebase_graph-*")):
        if not candidate.is_file() or not os.access(candidate, os.X_OK):
            continue
        try:
            listed = subprocess.run([str(candidate.resolve()), "--list", "--format", "terse"],
                                    env=env, capture_output=True, text=True, timeout=15)
            contains_case = args.case + ": test" in listed.stdout.splitlines()
            inventory.append({"path": str(candidate), "exit_code": listed.returncode,
                              "contains_exact_case": contains_case})
            if listed.returncode == 0 and contains_case:
                if selected is not None:
                    raise RuntimeError("More than one existing binary contains the exact test; refusing to guess")
                selected = candidate.resolve()
        except subprocess.TimeoutExpired:
            inventory.append({"path": str(candidate), "inventory_timeout": True})
    write_json(output / "binary-inventory.json", inventory)
    if selected is None:
        summary["unavailable"] = "No existing executable contains the requested exact library test"
        write_json(output / "summary.json", summary)
        return
    summary["binary"] = str(selected)
    summary["binary_sha256"] = digest(selected)
    rustc = subprocess.run(["rustc", "-Vv"], capture_output=True, text=True, check=False)
    (output / "rustc-version.txt").write_text(rustc.stdout + rustc.stderr)
    sources = [Path("Cargo.toml"), Path("Cargo.lock"), Path(".cargo/config.toml")]
    sources.extend(sorted(Path("src").rglob("*.rs")))
    write_json(output / "source-hashes.json", {str(path): digest(path) for path in sources if path.is_file()})
    commands = ["set pagination off", "set confirm off", "set print frame-arguments none",
                "set print thread-events off", "set disable-randomization off", "set startup-with-shell off",
                "handle SIGSEGV stop print nopass", "handle SIGABRT stop print nopass"]
    if test_home is not None:
        commands.append("set environment HOME = " + test_home)
    commands.extend(["run",
                "thread apply all bt 128", "info registers", "info proc mappings", "python",
                "output_path = " + repr(str(output / "frames.json")), CAPTURE, "end", "quit"])
    command_file = output / "commands.gdb"
    command_file.write_text("\n".join(commands) + "\n")
    command = ["gdb", "-q", "-nx", "-nh", "--batch", "-x", str(command_file), "--args",
               str(selected), args.case, "--exact", "--nocapture", "--test-threads=1"]
    summary["command"] = command
    write_json(output / "summary.json", summary)
    # Diagnostic output only: prohibit large core files, leaving the debuggee's
    # process/thread stack limits unchanged. Timeout kills the entire group.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    started = time.monotonic()
    with (output / "gdb.txt").open("w") as log:
        process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            process.wait(timeout=args.timeout_seconds)
            summary["debugger_exit_code"] = process.returncode
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            summary["timeout"] = True
    summary["elapsed_seconds"] = time.monotonic() - started
    summary["core_dumps_disabled"] = True
    log_path = output / "gdb.txt"
    summary["debugger_log_bytes"] = log_path.stat().st_size
    if log_path.stat().st_size > 2 * 1024 * 1024:
        with log_path.open("rb") as log:
            head = log.read(64 * 1024)
            log.seek(-(2 * 1024 * 1024 - len(head) - 128), os.SEEK_END)
            tail = log.read()
        log_path.write_bytes(head + b"\n[diagnostic log truncated; final backtrace retained below]\n" + tail)
        summary["debugger_log_truncated"] = True
    summary["frames_captured"] = (output / "frames.json").is_file()
    write_json(output / "summary.json", summary)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
