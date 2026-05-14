from __future__ import annotations

import json
from pathlib import Path

from codebase_graph.cli import main

FIXTURE = Path(__file__).parent / "fixtures" / "sample_project"

def test_cli_status_schema_and_search(tmp_path: Path, capsys) -> None:
    state_dir = tmp_path / "graph"
    assert main(["--source-root", str(FIXTURE), "--state-dir", str(state_dir), "status"]) == 0
    status = json.loads(capsys.readouterr().out)
    assert status["stale"] is True
    assert main(["--source-root", str(FIXTURE), "--state-dir", str(state_dir), "schema"]) == 0
    schema = json.loads(capsys.readouterr().out)
    assert schema["ontology"] == "codebase_graph_v1"
    assert main(["--source-root", str(FIXTURE), "--state-dir", str(state_dir), "search", "SampleService"]) == 0
    search = json.loads(capsys.readouterr().out)
    assert search["count"] >= 1
