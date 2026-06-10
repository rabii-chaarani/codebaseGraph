from __future__ import annotations

import tempfile
from pathlib import Path

from codebase_graph.db import LadybugUnavailableError, create_ladybug_database


def validate_ladybug_runtime() -> None:
    """Validate ladybug runtime for setup workflow and client configuration.

    This executes the selected workflow and returns a process status code or result object.

    Raises:
        LadybugUnavailableError: Raised when validation or runtime preconditions fail.
    """
    try:
        import real_ladybug  # noqa: F401
    except ImportError as exc:
        raise LadybugUnavailableError(
            "LadyBugDB is required for codebaseGraph setup. Install a package build that includes `real_ladybug`."
        ) from exc

    with tempfile.TemporaryDirectory(prefix="codebase-graph-preflight-") as temp_dir:
        db_path = Path(temp_dir) / "preflight.ldb"
        store = create_ladybug_database(db_path, include_fts=False)
        store.close()
