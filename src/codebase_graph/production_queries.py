from __future__ import annotations

from collections import Counter
from typing import Any

from .ontology import schema_payload

def graph_schema() -> dict[str, Any]:
    return schema_payload()

def graph_query(core: Any, query: str, parameters: dict[str, Any] | None = None) -> dict[str, Any]:
    return core.cypher(query, parameters=parameters or {})

def graph_coverage(core: Any) -> dict[str, Any]:
    core.ensure_current()
    graph = core._read_graph()
    counts = Counter(node.get("table", "Unknown") for node in graph.get("nodes", []))
    return {"node_counts": dict(counts), "node_count": sum(counts.values())}

def repository_analysis(core: Any) -> dict[str, Any]:
    search = core.search("project repository python module class function", limit=20)
    return {"retrieval": search.get("retrieval"), "items": search.get("items", []), "count": search.get("count", 0)}

def risk_report(core: Any) -> dict[str, Any]:
    result = core.cypher("MATCH (n:Risk) RETURN n.id, n.label, n.summary LIMIT 25")
    return {"items": result.get("rows", []), "count": result.get("count", 0)}

def task_report(core: Any) -> dict[str, Any]:
    return {"items": [], "count": 0}

def artifact_by_id(core: Any, artifact_id: str) -> dict[str, Any] | None:
    core.ensure_current()
    for node in core._read_graph().get("nodes", []):
        if node.get("id") == artifact_id:
            return node
    return None

def explain_decision(core: Any, decision_id: str) -> dict[str, Any]:
    artifact = artifact_by_id(core, decision_id)
    return {"id": decision_id, "artifact": artifact, "explanation": artifact.get("summary") if artifact else ""}
