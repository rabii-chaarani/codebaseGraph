from __future__ import annotations

import json
import os
import sys
from datetime import datetime, timezone
from typing import Any

LOG_LEVEL_ENV = "CODEBASE_GRAPH_LOG_LEVEL"
_LEVELS = {
    "DEBUG": 10,
    "INFO": 20,
    "WARNING": 30,
    "ERROR": 40,
    "CRITICAL": 50,
}


def log_event(event: str, *, level: str = "INFO", **fields: Any) -> None:
    """Write event for codebase graph runtime.

    This appends structured diagnostic data when diagnostics are enabled.

    Args:
        event: Event used by the codebase graph runtime workflow.
        level: Level used by the codebase graph runtime workflow.
        fields: Field mapping to read or serialize.
    """
    normalized_level = level.upper()
    if _LEVELS.get(normalized_level, 20) < _configured_level():
        return
    payload = {
        "event": event,
        "level": normalized_level,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        **_safe_fields(fields),
    }
    print(json.dumps(payload, separators=(",", ":"), sort_keys=True), file=sys.stderr)


def _configured_level() -> int:
    """Manage level within codebase graph runtime.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    configured = os.environ.get(LOG_LEVEL_ENV, "WARNING").upper()
    return _LEVELS.get(configured, _LEVELS["WARNING"])


def _safe_fields(fields: dict[str, Any]) -> dict[str, Any]:
    """Sanitize fields for codebase graph runtime.

    Args:
        fields: Field mapping to read or serialize.

    Returns:
        Structured mapping that follows the codebase graph runtime response contract.
    """
    safe: dict[str, Any] = {}
    for key, value in fields.items():
        if value is None or isinstance(value, (str, int, float, bool)):
            safe[key] = value
        elif isinstance(value, (list, tuple)):
            safe[key] = [_safe_value(item) for item in value]
        elif isinstance(value, dict):
            safe[key] = {str(item_key): _safe_value(item_value) for item_key, item_value in value.items()}
        else:
            safe[key] = str(value)
    return safe


def _safe_value(value: Any) -> Any:
    """Sanitize value for codebase graph runtime.

    Args:
        value: Input being normalized for serialization or validation.

    Returns:
        Any instance populated with data from the codebase graph runtime workflow.
    """
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, (list, tuple)):
        return [_safe_value(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _safe_value(item) for key, item in value.items()}
    return str(value)
