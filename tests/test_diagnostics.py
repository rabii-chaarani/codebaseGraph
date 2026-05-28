from __future__ import annotations

import json

from codebase_graph.diagnostics import LOG_LEVEL_ENV, log_event


def test_log_event_emits_json_to_stderr_when_level_allows(
    monkeypatch,
    capsys,
) -> None:
    monkeypatch.setenv(LOG_LEVEL_ENV, "INFO")

    log_event("sample.event", level="INFO", count=2, payload={"ok": True})

    captured = capsys.readouterr()
    assert captured.out == ""
    event = json.loads(captured.err)
    assert event["event"] == "sample.event"
    assert event["level"] == "INFO"
    assert event["count"] == 2
    assert event["payload"] == {"ok": True}
    assert event["timestamp"]


def test_log_event_respects_configured_level(monkeypatch, capsys) -> None:
    monkeypatch.setenv(LOG_LEVEL_ENV, "ERROR")

    log_event("sample.event", level="INFO")

    assert capsys.readouterr().err == ""
