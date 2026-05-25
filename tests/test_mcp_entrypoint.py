from __future__ import annotations

import subprocess
import sys


def test_mcp_entrypoint_help_imports_without_setup_cycle() -> None:
    completed = subprocess.run(
        [
            sys.executable,
            "-c",
            "from codebase_graph.mcp.server import main; raise SystemExit(main())",
            "--help",
        ],
        capture_output=True,
        text=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr
    assert "usage: codebase-graph-mcp" in completed.stdout
