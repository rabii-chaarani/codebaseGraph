from __future__ import annotations

import re
from typing import Any

def summarize_verification_run(command: str, output: str, exit_code: int | None = None) -> dict[str, Any]:
    status = "passed" if exit_code == 0 else "failed" if exit_code else "unknown"
    return {
        "command": command,
        "status": status,
        "exit_code": exit_code,
        "summary": _compact_output(output),
        "tool": _tool_name_from_command(command),
    }

def redact_verification_text(text: str) -> str:
    text = re.sub(r"(?i)(api[_-]?key|token|secret|password)=\S+", r"\1=<redacted>", text)
    return text

def _compact_output(output: str, limit: int = 1200) -> str:
    cleaned = redact_verification_text(output).strip()
    if len(cleaned) <= limit:
        return cleaned
    return f"{cleaned[:limit].rstrip()}..."

def _tool_name_from_command(command: str) -> str:
    parts = command.strip().split()
    if not parts:
        return "unknown"
    if parts[0] in {"python", "python3"} and len(parts) > 2 and parts[1] == "-m":
        return parts[2]
    return parts[0]
