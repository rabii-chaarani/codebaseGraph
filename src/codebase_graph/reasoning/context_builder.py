from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
import re
from typing import Any

from codebase_graph.db import graph_query_adapter
from codebase_graph.ontology import CONTEXT_PROFILES, RELATION_TYPES


DEFAULT_CONTEXT_LIMIT = 3
DEFAULT_CONTEXT_BUDGET = 600
SEMANTIC_CONTEXT_RELATIONS = {"Calls", "References", "ResolvesTo", "HasTypeAnnotation", "EvidencedBy"}
TOKEN_RE = re.compile(r"[A-Za-z0-9_]+|[^\sA-Za-z0-9_]")
SECRET_LITERAL_RE = re.compile(r"\b(?:sk-[A-Za-z0-9_\-]{10,}|ghp_[A-Za-z0-9_]{20,}|xox[baprs]-[A-Za-z0-9-]{10,})\b")
SECRET_ASSIGNMENT_RE = re.compile(
    r"(?i)\b([A-Z0-9_]*(?:API[_-]?KEY|TOKEN|SECRET|PASSWORD|PASSWD|PWD)[A-Z0-9_]*)\b(\s*[:=]\s*)([^\s#]+)"
)


@dataclass(frozen=True, slots=True)
class SemanticRelationAnnotation:
    """Expose semantic relation confidence and evidence in graph context output."""

    relation_kind: str
    confidence: float | None = None
    provider: str = ""
    evidence_ids: tuple[str, ...] = ()
    diagnostics: tuple[str, ...] = ()
    metadata: dict[str, Any] = field(default_factory=dict)

    def as_dict(
        self,
        *,
        include_confidence: bool = True,
        include_evidence: bool = False,
        include_provider: bool = True,
    ) -> dict[str, Any]:
        """Serialize this annotation for CLI and MCP graph payloads."""
        payload: dict[str, Any] = {"relation_kind": self.relation_kind}
        if include_confidence and self.confidence is not None:
            payload["confidence"] = self.confidence
        if include_provider and self.provider:
            payload["provider"] = self.provider
        if include_evidence:
            if self.evidence_ids:
                payload["evidence_ids"] = list(self.evidence_ids)
            if self.diagnostics:
                payload["diagnostics"] = list(self.diagnostics)
            if self.metadata:
                payload["metadata"] = dict(self.metadata)
        return payload


@dataclass(frozen=True, slots=True)
class ContextPathNode:
    """Identify one node participating in an evidence path."""
    id: str
    type: str
    label: str

    def as_dict(self) -> dict[str, str]:
        """Serialize this path node for structured output."""
        return {"id": self.id, "type": self.type, "label": self.label}


@dataclass(frozen=True, slots=True)
class ContextEdge:
    """Describe one relation hop in a compact context evidence path."""
    relation: str
    direction: str
    source_node_id: str
    target_node_id: str
    edge_id: str = ""
    kind: str = ""
    confidence: float | None = None
    metadata: dict[str, Any] = field(default_factory=dict)

    def as_dict(
        self,
        *,
        detail: str = "standard",
        include_confidence: bool = True,
        include_evidence: bool | None = None,
    ) -> dict[str, Any]:
        """Serialize relation metadata without removing the existing context row shape."""
        include_evidence = detail != "slim" if include_evidence is None else include_evidence
        payload: dict[str, Any] = {
            "relation": self.relation,
            "direction": self.direction,
            "source_node_id": self.source_node_id,
            "target_node_id": self.target_node_id,
        }
        if self.edge_id:
            payload["edge_id"] = self.edge_id
        if self.kind:
            payload["kind"] = self.kind
        if include_confidence and self.confidence is not None:
            payload["confidence"] = self.confidence
        if include_evidence and self.metadata:
            payload["metadata"] = dict(self.metadata)
        return payload


@dataclass(frozen=True, slots=True)
class ContextPath:
    """Represent the relation chain that explains why a context node was returned."""
    nodes: tuple[ContextPathNode, ...] = ()
    edges: tuple[ContextEdge, ...] = ()

    def extend(self, edge: ContextEdge, node: ContextPathNode) -> ContextPath:
        """Return a new evidence path with one additional relation hop."""
        return ContextPath(nodes=(*self.nodes, node), edges=(*self.edges, edge))

    @property
    def relation_chain(self) -> tuple[str, ...]:
        """Return relation/direction identifiers used for path-level deduplication."""
        return tuple(f"{edge.direction}:{edge.relation}" for edge in self.edges)

    @property
    def chain(self) -> str:
        """Return a concise human-readable evidence chain."""
        if not self.nodes:
            return ""
        parts = [_path_node_label(self.nodes[0])]
        for edge, node in zip(self.edges, self.nodes[1:], strict=False):
            parts.extend((edge.relation, _path_node_label(node)))
        return " ".join(part for part in parts if part)

    def as_dict(
        self,
        *,
        detail: str = "standard",
        include_confidence: bool = True,
        include_evidence: bool | None = None,
    ) -> dict[str, Any]:
        """Serialize the evidence path for CLI and MCP payloads."""
        include_evidence = detail != "slim" if include_evidence is None else include_evidence
        payload: dict[str, Any] = {"chain": self.chain}
        if detail != "slim":
            payload["nodes"] = [node.as_dict() for node in self.nodes]
        payload["edges"] = [
            edge.as_dict(
                detail=detail,
                include_confidence=include_confidence,
                include_evidence=include_evidence,
            )
            for edge in self.edges
        ]
        return payload


@dataclass(frozen=True, slots=True)
class SourceSnippet:
    """Carry optional source evidence attached to a context node."""
    path: str
    span: dict[str, int]
    text: str
    redactions: tuple[str, ...] = ()

    def as_dict(self, *, detail: str = "standard") -> dict[str, Any]:
        """Serialize the snippet after redaction."""
        payload: dict[str, Any] = {
            "path": self.path,
            "span": dict(self.span),
            "text": self.text,
        }
        if self.redactions:
            payload["redactions"] = list(self.redactions)
        return payload


@dataclass(frozen=True, slots=True)
class ContextNode:
    """Represent context node data used by graph context and architecture-query reasoning.

    The class belongs to Compact graph-neighborhood builder used to explain search results under
    a token budget.
    """
    relation: str
    direction: str
    type: str
    label: str
    path: str = ""
    span: dict[str, int] = field(default_factory=dict)
    summary: str = ""
    id: str = field(default="", repr=False)
    evidence_path: ContextPath | None = None
    snippet: SourceSnippet | None = None

    def as_dict(
        self,
        *,
        detail: str = "standard",
        include_semantic: bool = True,
        include_confidence: bool = True,
        include_evidence: bool | None = None,
    ) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Args:
            detail: Response detail level requested by CLI or MCP callers.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if detail not in {"standard", "slim"}:
            raise ValueError(f"Unknown detail level: {detail}. Valid levels: slim, standard")
        include_evidence = detail != "slim" if include_evidence is None else include_evidence
        if detail == "slim":
            payload: dict[str, Any] = {
                "relation": self.relation,
                "direction": self.direction,
                "type": self.type,
                "label": self.label,
            }
            if self.path:
                payload["path"] = self.path
            if self.span:
                payload["span"] = dict(self.span)
            if self.summary and self.summary != self.label:
                payload["summary"] = self.summary
            if self.evidence_path is not None:
                payload["evidence_path"] = self.evidence_path.as_dict(
                    detail=detail,
                    include_confidence=include_confidence,
                    include_evidence=include_evidence,
                )
            if include_semantic:
                _set_semantic_annotations(
                    payload,
                    self.evidence_path,
                    include_provider=False,
                    include_confidence=include_confidence,
                    include_evidence=include_evidence,
                )
            if self.snippet is not None:
                payload["snippet"] = self.snippet.as_dict(detail=detail)
            return payload
        payload = {
            "relation": self.relation,
            "direction": self.direction,
            "type": self.type,
            "label": self.label,
            "path": self.path,
            "span": dict(self.span),
            "summary": self.summary,
        }
        if self.evidence_path is not None:
            payload["evidence_path"] = self.evidence_path.as_dict(
                detail=detail,
                include_confidence=include_confidence,
                include_evidence=include_evidence,
            )
        if include_semantic:
            _set_semantic_annotations(
                payload,
                self.evidence_path,
                include_provider=True,
                include_confidence=include_confidence,
                include_evidence=include_evidence,
            )
        if self.snippet is not None:
            payload["snippet"] = self.snippet.as_dict(detail=detail)
        return payload


class CompactContextBuilder:
    """Traverse graph relations to produce compact context for agents."""
    def __init__(
        self,
        store: Any,
        *,
        repo_root: str | Path | None = None,
        profile_catalog: dict[str, Any] | None = None,
    ) -> None:
        """Initialize compact context builder with the collaborators and state it owns.

        Args:
            store: Graph store used for persistence or read-only queries.
        """
        self.store = store
        self.query = graph_query_adapter(store)
        self._relation_names = {relation_type.name for relation_type in RELATION_TYPES}
        self.repo_root = Path(repo_root).resolve() if repo_root is not None else None
        self.profile_catalog = profile_catalog or CONTEXT_PROFILES

    def build(
        self,
        node_id: str,
        node_type: str,
        *,
        profile: str = "brief",
        limit: int = DEFAULT_CONTEXT_LIMIT,
        budget: int = DEFAULT_CONTEXT_BUDGET,
        max_depth: int | None = None,
        root_label: str = "",
        include_snippets: bool = False,
        snippet_context_lines: int = 0,
    ) -> list[ContextNode]:
        """Build a budgeted graph-neighborhood explanation for a selected node.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            profile: Context profile controlling graph-neighborhood traversal.
            limit: Maximum number of rows or results requested.
            budget: Approximate token budget available for compact context.
            max_depth: Optional traversal depth limit for graph context.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        profile_config = self._profile(profile)
        if limit <= 0 or budget <= 0:
            return []
        depth_limit = profile_config["max_depth"] if max_depth is None else max_depth
        if depth_limit <= 0:
            return []

        relations = tuple(
            relation
            for relation in profile_config["relations"]
            if relation in self._relation_names
        )
        if not relations:
            return []

        context: list[ContextNode] = []
        seen = {_path_dedupe_key(node_id, ())}
        root_path = ContextPath(nodes=(ContextPathNode(node_id, node_type, root_label or node_id),))
        frontier = [(node_id, node_type, 0, root_path)]
        used_budget = 0

        while frontier and len(context) < limit:
            current_id, current_type, depth, current_path = frontier.pop(0)
            if depth >= depth_limit:
                continue
            for relation in relations:
                candidates = self._neighbors(current_id, current_type, relation, limit, current_path)
                if relation in SEMANTIC_CONTEXT_RELATIONS:
                    candidates = prioritize_semantic_context(candidates)
                for candidate in candidates:
                    if candidate.type == "" or candidate.label == "":
                        continue
                    node_key = _node_key(candidate)
                    dedupe_key = _context_dedupe_key(candidate)
                    if dedupe_key in seen:
                        continue
                    if include_snippets:
                        candidate = _with_snippet(candidate, self.repo_root, context_lines=snippet_context_lines)
                    compact_candidate, item_cost = _fit_to_budget(candidate, budget - used_budget)
                    if compact_candidate is None:
                        return context
                    context.append(compact_candidate)
                    used_budget += item_cost
                    seen.add(dedupe_key)
                    if node_key:
                        frontier.append((node_key, candidate.type, depth + 1, candidate.evidence_path or current_path))
                    if len(context) >= limit:
                        return context
        return context

    def _profile(self, profile: str) -> dict[str, Any]:
        """Manage graph context and architecture-query reasoning state.

        Args:
            profile: Context profile controlling graph-neighborhood traversal.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.

        Raises:
            ValueError: Raised when validation or runtime preconditions fail.
        """
        if profile not in self.profile_catalog:
            valid = ", ".join(sorted(self.profile_catalog))
            raise ValueError(f"Unknown context profile: {profile}. Valid profiles: {valid}")
        return dict(self.profile_catalog[profile])

    def _neighbors(
        self,
        node_id: str,
        node_type: str,
        relation: str,
        limit: int,
        current_path: ContextPath,
    ) -> list[ContextNode]:
        """Manage graph context and architecture-query reasoning state.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        outgoing = self._query_neighbors(node_id, node_type, relation, "outgoing", limit, current_path)
        incoming = self._query_neighbors(node_id, node_type, relation, "incoming", limit, current_path)
        return [*outgoing, *incoming]

    def _query_neighbors(
        self,
        node_id: str,
        node_type: str,
        relation: str,
        direction: str,
        limit: int,
        current_path: ContextPath,
    ) -> list[ContextNode]:
        """Build neighbors for graph context and architecture-query reasoning.

        Args:
            node_id: Identifier for the node graph object.
            node_type: Ontology node type used to choose a table or label.
            relation: Ontology relation name used for graph traversal.
            direction: Traversal direction relative to the source node.
            limit: Maximum number of rows or results requested.

        Returns:
            Ordered results returned to the graph context and architecture-query reasoning
            caller.
        """
        context: list[ContextNode] = []
        for neighbor in self.query.neighbors(
            node_id=node_id,
            node_type=node_type,
            relation=relation,
            direction=direction,
            limit=limit,
        ):
            edge = ContextEdge(
                relation=neighbor.relation or relation,
                direction=neighbor.direction or direction,
                source_node_id=neighbor.source_node_id or (node_id if direction == "outgoing" else neighbor.node_id),
                target_node_id=neighbor.target_node_id or (neighbor.node_id if direction == "outgoing" else node_id),
                edge_id=neighbor.edge_id,
                kind=neighbor.edge_kind,
                confidence=neighbor.edge_confidence,
                metadata=neighbor.edge_metadata,
            )
            evidence_path = current_path.extend(
                edge,
                ContextPathNode(
                    neighbor.node_id,
                    neighbor.node_type,
                    neighbor.label or neighbor.qualified_name or neighbor.node_id,
                ),
            )
            context.append(
                ContextNode(
                    relation=relation,
                    direction=direction,
                    type=neighbor.node_type,
                    label=neighbor.label or neighbor.qualified_name,
                    path=neighbor.path,
                    span=_span(neighbor.line_start, neighbor.line_end),
                    summary=neighbor.summary,
                    id=neighbor.node_id,
                    evidence_path=evidence_path,
                )
            )
        return context


def extract_semantic_relation_annotations(
    evidence_path: ContextPath | None,
    *,
    include_confidence: bool = True,
    include_evidence: bool = False,
) -> tuple[SemanticRelationAnnotation, ...]:
    """Build semantic relation annotations from relation metadata on context hops."""
    if evidence_path is None:
        return ()
    annotations = tuple(
        annotation
        for edge in evidence_path.edges
        if (annotation := _semantic_annotation_for_edge(edge)) is not None
    )
    if include_confidence and include_evidence:
        return annotations
    return tuple(
        SemanticRelationAnnotation(
            annotation.relation_kind,
            annotation.confidence if include_confidence else None,
            annotation.provider,
            annotation.evidence_ids if include_evidence else (),
            annotation.diagnostics if include_evidence else (),
            annotation.metadata if include_evidence else {},
        )
        for annotation in annotations
    )


def prioritize_semantic_context(nodes: list[ContextNode]) -> list[ContextNode]:
    """Return context nodes with semantic relation evidence first."""
    return sorted(
        nodes,
        key=lambda node: (
            not bool(extract_semantic_relation_annotations(node.evidence_path)),
            node.relation,
            node.direction,
            node.type,
            node.label,
        ),
    )


def _fit_to_budget(node: ContextNode, remaining_budget: int) -> tuple[ContextNode | None, int]:
    """Manage to budget within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.
        remaining_budget: Remaining budget used by the graph context and architecture-
        query reasoning workflow.

    Returns:
        Tuple of stable results returned to the graph context and architecture-query
        reasoning caller.
    """
    cost = _context_cost(node)
    if cost <= remaining_budget:
        return node, cost
    fixed = ContextNode(
        node.relation,
        node.direction,
        node.type,
        node.label,
        node.path,
        node.span,
        "",
        node.id,
        node.evidence_path,
        node.snippet,
    )
    fixed_cost = _context_cost(fixed)
    summary_budget = remaining_budget - fixed_cost
    if summary_budget <= 0:
        return None, 0
    # Keep the relationship identity and source location intact, then trim only
    # the summary text because it is the least structural part of the context row.
    summary = _trim_text_to_token_budget(node.summary, summary_budget)
    compact = ContextNode(
        node.relation,
        node.direction,
        node.type,
        node.label,
        node.path,
        node.span,
        summary,
        node.id,
        node.evidence_path,
        node.snippet,
    )
    return compact, _context_cost(compact)


def _context_cost(node: ContextNode) -> int:
    """Manage cost within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Integer count, status code, or index used by the caller.
    """
    values: list[Any] = [
        node.relation,
        node.direction,
        node.type,
        node.label,
        node.path,
        node.summary,
        *node.span.values(),
    ]
    if node.evidence_path is not None:
        values.append(node.evidence_path.chain)
        for edge in node.evidence_path.edges:
            values.extend(
                (
                    edge.relation,
                    edge.direction,
                    edge.source_node_id,
                    edge.target_node_id,
                    edge.edge_id,
                    edge.kind,
                    edge.confidence,
                    edge.metadata,
                )
            )
    if node.snippet is not None:
        values.extend((node.snippet.path, *node.snippet.span.values(), node.snippet.text, *node.snippet.redactions))
    return sum(estimate_token_count(str(value)) for value in values if value not in ("", None, {}, []))


def estimate_token_count(text: str) -> int:
    """Estimate LLM token cost without requiring a tokenizer dependency."""
    return len(TOKEN_RE.findall(text))


def _trim_text_to_token_budget(text: str, budget: int) -> str:
    """Trim text to an approximate token count while preserving original spacing."""
    if budget <= 0:
        return ""
    matches = list(TOKEN_RE.finditer(text))
    if len(matches) <= budget:
        return text
    return text[: matches[budget - 1].end()].rstrip()


def _with_snippet(node: ContextNode, repo_root: Path | None, *, context_lines: int) -> ContextNode:
    """Attach an optional redacted source snippet when source bounds are available."""
    snippet = collect_source_snippet(repo_root, node.path, node.span, context_lines=max(context_lines, 0))
    if snippet is None:
        return node
    return ContextNode(
        node.relation,
        node.direction,
        node.type,
        node.label,
        node.path,
        node.span,
        node.summary,
        node.id,
        node.evidence_path,
        snippet,
    )


def collect_source_snippet(
    repo_root: str | Path | None,
    path: str,
    span: dict[str, int],
    *,
    context_lines: int = 0,
) -> SourceSnippet | None:
    """Collect and redact a bounded snippet from a repository-relative source path."""
    if repo_root is None or not path or not span:
        return None
    root = Path(repo_root).resolve()
    source_path = (root / path).resolve()
    try:
        source_path.relative_to(root)
    except ValueError:
        return None
    if not source_path.is_file():
        return None
    start = span.get("line_start")
    end = span.get("line_end")
    if start is None or end is None:
        return None
    lines = source_path.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True)
    if not lines:
        return None
    start_line = max(1, int(start) - context_lines)
    end_line = min(len(lines), int(end) + context_lines)
    if start_line > end_line:
        return None
    text = "".join(lines[start_line - 1 : end_line])
    redacted, redactions = redact_source_snippet(text)
    return SourceSnippet(
        path=path,
        span={"line_start": start_line, "line_end": end_line},
        text=redacted,
        redactions=tuple(redactions),
    )


def redact_source_snippet(text: str) -> tuple[str, list[str]]:
    """Redact secret-like literals before snippets reach JSON or block output."""
    redactions: list[str] = []

    def redact_literal(match: re.Match[str]) -> str:
        redactions.append("token")
        return "[REDACTED]"

    def redact_assignment(match: re.Match[str]) -> str:
        redactions.append(match.group(1))
        return f"{match.group(1)}{match.group(2)}[REDACTED]"

    redacted = SECRET_LITERAL_RE.sub(redact_literal, text)
    redacted = SECRET_ASSIGNMENT_RE.sub(redact_assignment, redacted)
    return redacted, redactions


def _node_key(node: ContextNode) -> str:
    """Manage key within graph context and architecture-query reasoning.

    Args:
        node: Parser or graph node being inspected.

    Returns:
        Formatted text returned to the caller.
    """
    return node.id


def _context_dedupe_key(node: ContextNode) -> str:
    """Dedupe by terminal node and relation chain."""
    node_key = _node_key(node) or f"{node.type}:{node.label}:{node.path}:{node.span}"
    relation_chain = node.evidence_path.relation_chain if node.evidence_path is not None else ()
    return _path_dedupe_key(node_key, relation_chain)


def _path_dedupe_key(node_key: str, relation_chain: tuple[str, ...]) -> str:
    """Build a stable key for a terminal node plus relation chain."""
    return f"{node_key}|{'/'.join(relation_chain)}"


def _path_node_label(node: ContextPathNode) -> str:
    """Return a concise label for a node inside an evidence chain."""
    if node.label:
        return f"{node.type} {node.label}"
    return node.type


def _span(line_start: Any, line_end: Any) -> dict[str, int]:
    """Manage graph context and architecture-query reasoning state.

    Args:
        line_start: Start line from parser or database metadata.
        line_end: End line from parser or database metadata.

    Returns:
        Structured mapping that follows the graph context and architecture-query
        reasoning response contract.
    """
    span: dict[str, int] = {}
    if line_start is not None:
        span["line_start"] = int(line_start)
    if line_end is not None:
        span["line_end"] = int(line_end)
    return span


def _set_semantic_annotations(
    payload: dict[str, Any],
    evidence_path: ContextPath | None,
    *,
    include_provider: bool,
    include_confidence: bool,
    include_evidence: bool,
) -> None:
    annotations = extract_semantic_relation_annotations(
        evidence_path,
        include_confidence=include_confidence,
        include_evidence=include_evidence,
    )
    if annotations:
        payload["semantic_annotations"] = [
            annotation.as_dict(
                include_confidence=include_confidence,
                include_evidence=include_evidence,
                include_provider=include_provider,
            )
            for annotation in annotations
        ]


def _semantic_annotation_for_edge(edge: ContextEdge) -> SemanticRelationAnnotation | None:
    if not _is_semantic_edge(edge):
        return None
    metadata = dict(edge.metadata)
    return SemanticRelationAnnotation(
        relation_kind=edge.kind,
        confidence=edge.confidence,
        provider=_semantic_provider(metadata),
        evidence_ids=_semantic_evidence_ids(edge, metadata),
        diagnostics=_semantic_diagnostics(metadata),
        metadata=metadata,
    )


def _is_semantic_edge(edge: ContextEdge) -> bool:
    if edge.kind.startswith(("semantic", "provider")):
        return True
    resolver = str(edge.metadata.get("resolver") or "")
    if resolver.startswith(("semantic", "provider")):
        return True
    return bool(edge.metadata.get("resolution_source") or edge.metadata.get("confidence_source"))


def _semantic_provider(metadata: dict[str, Any]) -> str:
    for key in ("provider", "resolution_source", "confidence_source", "resolver", "source"):
        value = metadata.get(key)
        if value:
            return str(value)
    return ""


def _semantic_evidence_ids(edge: ContextEdge, metadata: dict[str, Any]) -> tuple[str, ...]:
    values: list[str] = []
    for value in (edge.edge_id, metadata.get("evidence_id"), metadata.get("source_edge"), metadata.get("edge_id")):
        if value:
            values.append(str(value))
    return tuple(dict.fromkeys(values))


def _semantic_diagnostics(metadata: dict[str, Any]) -> tuple[str, ...]:
    diagnostics = metadata.get("diagnostics", ())
    if isinstance(diagnostics, str):
        return (diagnostics,)
    if isinstance(diagnostics, list | tuple):
        return tuple(str(item) for item in diagnostics if item)
    return ()


__all__ = [
    "CompactContextBuilder",
    "ContextEdge",
    "ContextNode",
    "ContextPath",
    "ContextPathNode",
    "SemanticRelationAnnotation",
    "SourceSnippet",
    "DEFAULT_CONTEXT_BUDGET",
    "DEFAULT_CONTEXT_LIMIT",
    "collect_source_snippet",
    "estimate_token_count",
    "extract_semantic_relation_annotations",
    "prioritize_semantic_context",
    "redact_source_snippet",
]
