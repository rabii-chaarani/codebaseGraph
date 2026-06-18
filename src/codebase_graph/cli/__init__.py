from __future__ import annotations

import subprocess
import sys
from collections.abc import Sequence

from codebase_graph.native_binary import resolve_native_product_binary


def main(argv: Sequence[str] | None = None) -> int:
    argv_list = list(argv) if argv is not None else sys.argv[1:]
    return _run_native_entrypoint(argv_list)


def _run_native_entrypoint(argv: Sequence[str]) -> int:
    native_binary = _native_product_binary()
    if native_binary is None:
        raise SystemExit(
            "Rust native CLI binary is required. Build or install `codebase-graph`, "
            "or set CODEBASE_GRAPH_NATIVE_CLI to its absolute path."
        )
    if _requires_process_stdio(argv):
        status = subprocess.call([native_binary, *argv])
        if status:
            raise SystemExit(status)
        return status
    return _run_native_binary_command(native_binary, argv)


def _requires_process_stdio(argv: Sequence[str]) -> bool:
    if len(argv) < 2 or argv[0] != "mcp":
        return False
    return argv[1] in {"serve", "http"}


def _run_native_binary_command(native_binary: str, argv: Sequence[str]) -> int:
    completed = subprocess.run([native_binary, *argv], capture_output=True, text=True, check=False)
    if completed.stdout:
        print(completed.stdout, end="")
    if completed.stderr:
        print(completed.stderr, end="", file=sys.stderr)
    if completed.returncode:
        raise SystemExit(completed.returncode)
    return completed.returncode


def _native_product_binary() -> str | None:
    return resolve_native_product_binary(skip_current_script=True)
