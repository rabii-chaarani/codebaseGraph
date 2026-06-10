from __future__ import annotations

from dataclasses import dataclass
from typing import Any

WORKFLOW_NAME = "coding_task_architecture_discovery"
EXECUTION_TOOL = "graph_query"


@dataclass(frozen=True, slots=True)
class ArchitectureQuerySpec:
    """Describe a declared architecture query used by graph context and architecture-query reasoning.

    The class belongs to Architecture-discovery Cypher catalog exposed to coding agents.
    """
    name: str
    description: str
    statement: str
    parameters: tuple[str, ...] = ()
    returns: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.
        """
        return {
            "name": self.name,
            "description": self.description,
            "statement": self.statement,
            "parameters": list(self.parameters),
            "returns": list(self.returns),
        }


@dataclass(frozen=True, slots=True)
class ArchitectureQueryGroup:
    """Represent architecture query group data used by graph context and architecture-query reasoning.
    """
    name: str
    goal: str
    queries: tuple[ArchitectureQuerySpec, ...]

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the graph context and architecture-query
            reasoning response contract.
        """
        return {
            "name": self.name,
            "goal": self.goal,
            "queries": [query.as_dict() for query in self.queries],
        }


ARCHITECTURE_QUERY_ORDER = (
    "overview",
    "public_surface",
    "dependency_topology",
    "execution_flow",
    "runtime_data_security",
    "documentation_context",
    "graph_quality_gaps",
)


ARCHITECTURE_QUERY_GROUPS: dict[str, ArchitectureQueryGroup] = {
    "overview": ArchitectureQueryGroup(
        name="overview",
        goal="Check graph coverage and establish the indexed codebase shape.",
        queries=(
            ArchitectureQuerySpec(
                name="graph_coverage",
                description="Count all materialized graph nodes as a quick coverage check.",
                statement="MATCH (n) RETURN count(n) AS total_nodes LIMIT 1",
                returns=("total_nodes",),
            ),
            ArchitectureQuerySpec(
                name="source_unit_inventory",
                description="List materialized modules with source paths and spans.",
                statement=(
                    "MATCH (m:Module) "
                    "RETURN m.id, m.label, m.qualified_name, m.path, m.line_start, m.line_end "
                    "ORDER BY m.path LIMIT 200"
                ),
                returns=("id", "label", "qualified_name", "path", "line_start", "line_end"),
            ),
            ArchitectureQuerySpec(
                name="package_directory_shape",
                description="List source files with path and content metadata.",
                statement=(
                    "MATCH (f:File) "
                    "RETURN f.path, f.label, f.size_bytes, f.content_hash "
                    "ORDER BY f.path LIMIT 300"
                ),
                returns=("path", "label", "size_bytes", "content_hash"),
            ),
        ),
    ),
    "public_surface": ArchitectureQueryGroup(
        name="public_surface",
        goal="Find how the library exposes behavior through modules, definitions, or runtime entrypoints.",
        queries=(
            ArchitectureQuerySpec(
                name="public_surface_candidates",
                description="Find exposed module surfaces and fallback definition-level public candidates.",
                statement=(
                    "MATCH (m:Module)-[:FROM_Exposes]->(:Exposes)-[:TO_Exposes]->(surface) "
                    "RETURN 'exposed' AS surface_source, m.label AS module_label, m.path AS module_path, "
                    "surface.id AS surface_id, surface.label AS surface_label, "
                    "surface.qualified_name AS surface_qualified_name, surface.path AS surface_path, "
                    "surface.line_start AS line_start "
                    "UNION ALL "
                    "MATCH (m:Module)-[:FROM_Defines]->(:Defines)-[:TO_Defines]->(surface:Class) "
                    "RETURN 'defined' AS surface_source, m.label AS module_label, m.path AS module_path, "
                    "surface.id AS surface_id, surface.label AS surface_label, "
                    "surface.qualified_name AS surface_qualified_name, surface.path AS surface_path, "
                    "surface.line_start AS line_start "
                    "UNION ALL "
                    "MATCH (m:Module)-[:FROM_Defines]->(:Defines)-[:TO_Defines]->(surface:Function) "
                    "RETURN 'defined' AS surface_source, m.label AS module_label, m.path AS module_path, "
                    "surface.id AS surface_id, surface.label AS surface_label, "
                    "surface.qualified_name AS surface_qualified_name, surface.path AS surface_path, "
                    "surface.line_start AS line_start "
                    "UNION ALL "
                    "MATCH (m:Module)-[:FROM_Defines]->(:Defines)-[:TO_Defines]->(surface:Method) "
                    "RETURN 'defined' AS surface_source, m.label AS module_label, m.path AS module_path, "
                    "surface.id AS surface_id, surface.label AS surface_label, "
                    "surface.qualified_name AS surface_qualified_name, surface.path AS surface_path, "
                    "surface.line_start AS line_start LIMIT 200"
                ),
                returns=(
                    "surface_source",
                    "module_label",
                    "module_path",
                    "surface_id",
                    "surface_label",
                    "surface_qualified_name",
                    "surface_path",
                    "line_start",
                ),
            ),
            ArchitectureQuerySpec(
                name="entrypoint_runtime_surface",
                description="Find function-level name/path candidates for runtime or CLI entrypoints.",
                statement=(
                    "MATCH (d:Function) "
                    "WHERE d.label = 'main' OR d.label = 'cli' OR d.label CONTAINS 'server' OR d.path CONTAINS 'cli' "
                    "RETURN 'name_candidate' AS entrypoint_kind, d.id AS entrypoint_id, d.label AS entrypoint_label, "
                    "d.path AS entrypoint_path, d.id AS target_id, d.label AS target_label, "
                    "d.qualified_name AS target_qualified_name, d.path AS target_path, d.line_start AS line_start "
                    "LIMIT 100"
                ),
                returns=(
                    "entrypoint_kind",
                    "entrypoint_id",
                    "entrypoint_label",
                    "entrypoint_path",
                    "target_id",
                    "target_label",
                    "target_qualified_name",
                    "target_path",
                    "line_start",
                ),
            ),
        ),
    ),
    "dependency_topology": ArchitectureQueryGroup(
        name="dependency_topology",
        goal="Map internal and external dependencies so agents can infer layers and adapters.",
        queries=(
            ArchitectureQuerySpec(
                name="external_dependency_map",
                description="Map import declarations to external dependency nodes.",
                statement=(
                    "MATCH (i:ImportDeclaration)-[:FROM_DependsOn]->(:DependsOn)-[:TO_DependsOn]->(d:Dependency) "
                    "RETURN i.path, i.label AS import_label, d.label AS dependency "
                    "ORDER BY d.label, i.path LIMIT 300"
                ),
                returns=("path", "import_label", "dependency"),
            ),
            ArchitectureQuerySpec(
                name="module_import_coupling",
                description="List modules and their import declarations as a coupling inventory.",
                statement=(
                    "MATCH (m:Module)-[:FROM_Imports]->(:Imports)-[:TO_Imports]->(i:ImportDeclaration) "
                    "RETURN m.label, m.path, i.label, i.line_start "
                    "ORDER BY m.path, i.line_start LIMIT 300"
                ),
                returns=("module_label", "module_path", "import_label", "line_start"),
            ),
        ),
    ),
    "execution_flow": ArchitectureQueryGroup(
        name="execution_flow",
        goal="Identify important call paths, orchestration nodes, and central implementation flows.",
        queries=(
            ArchitectureQuerySpec(
                name="high_fan_in_definitions",
                description="Find definitions with many resolved incoming references.",
                statement=(
                    "MATCH (ref)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(target:Class) "
                    "RETURN target.id, target.label, target.qualified_name, target.path, count(ref) AS inbound_refs "
                    "UNION ALL "
                    "MATCH (ref)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(target:Function) "
                    "RETURN target.id, target.label, target.qualified_name, target.path, count(ref) AS inbound_refs "
                    "UNION ALL "
                    "MATCH (ref)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(target:Method) "
                    "RETURN target.id, target.label, target.qualified_name, target.path, count(ref) AS inbound_refs "
                    "UNION ALL "
                    "MATCH (ref)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(target:Module) "
                    "RETURN target.id, target.label, target.qualified_name, target.path, count(ref) AS inbound_refs "
                    "ORDER BY inbound_refs DESC LIMIT 50"
                ),
                returns=("id", "label", "qualified_name", "path", "inbound_refs"),
            ),
            ArchitectureQuerySpec(
                name="high_fan_out_callers",
                description="Find functions or methods that call many downstream nodes.",
                statement=(
                    "MATCH (caller:Function)-[:FROM_Calls]->(:Calls)-[:TO_Calls]->(callee) "
                    "RETURN caller.id, caller.label, caller.qualified_name, caller.path, count(callee) AS outgoing_calls "
                    "UNION ALL "
                    "MATCH (caller:Method)-[:FROM_Calls]->(:Calls)-[:TO_Calls]->(callee) "
                    "RETURN caller.id, caller.label, caller.qualified_name, caller.path, count(callee) AS outgoing_calls "
                    "ORDER BY outgoing_calls DESC LIMIT 50"
                ),
                returns=("id", "label", "qualified_name", "path", "outgoing_calls"),
            ),
            ArchitectureQuerySpec(
                name="callable_neighborhood",
                description="Inspect direct callees for a named callable.",
                statement=(
                    "MATCH (caller)-[:FROM_Calls]->(:Calls)-[:TO_Calls]->(callee) "
                    "WHERE caller.label = $name OR caller.qualified_name = $name "
                    "RETURN caller.id, caller.label, caller.qualified_name, callee.id, "
                    "callee.label, callee.qualified_name, callee.path LIMIT 100"
                ),
                parameters=("name",),
                returns=(
                    "caller_id",
                    "caller_label",
                    "caller_qualified_name",
                    "callee_id",
                    "callee_label",
                    "callee_qualified_name",
                    "callee_path",
                ),
            ),
        ),
    ),
    "runtime_data_security": ArchitectureQueryGroup(
        name="runtime_data_security",
        goal="Expose data access, query execution, secrets, and configuration-sensitive paths.",
        queries=(
            ArchitectureQuerySpec(
                name="data_query_touchpoints",
                description="Find actors that execute or construct query nodes.",
                statement=(
                    "MATCH (actor)-[:FROM_ExecutesQuery]->(:ExecutesQuery)-[:TO_ExecutesQuery]->(q:Query) "
                    "RETURN actor.id, actor.label, actor.qualified_name, actor.path, q.label, q.path, q.line_start "
                    "LIMIT 100"
                ),
                returns=(
                    "actor_id",
                    "actor_label",
                    "actor_qualified_name",
                    "actor_path",
                    "query_label",
                    "query_path",
                    "query_line_start",
                ),
            ),
            ArchitectureQuerySpec(
                name="secret_configuration_touchpoints",
                description="Find actors linked to secret or sensitive configuration references.",
                statement=(
                    "MATCH (actor)-[:FROM_UsesSecret]->(:UsesSecret)-[:TO_UsesSecret]->(s:SecretRef) "
                    "RETURN actor.id, actor.label, actor.qualified_name, actor.path, s.label, s.path, s.line_start "
                    "LIMIT 100"
                ),
                returns=(
                    "actor_id",
                    "actor_label",
                    "actor_qualified_name",
                    "actor_path",
                    "secret_label",
                    "secret_path",
                    "secret_line_start",
                ),
            ),
        ),
    ),
    "documentation_context": ArchitectureQueryGroup(
        name="documentation_context",
        goal="Link architecture claims to documentation and parser evidence.",
        queries=(
            ArchitectureQuerySpec(
                name="documentation_to_code_links",
                description="Find documentation chunks connected to code nodes.",
                statement=(
                    "MATCH (d:DocumentationChunk)-[:FROM_Documents]->(:Documents)-[:TO_Documents]->(n) "
                    "RETURN d.id, d.label, d.path, n.id, n.label, n.qualified_name, n.path LIMIT 100"
                ),
                returns=(
                    "doc_id",
                    "doc_label",
                    "doc_path",
                    "node_id",
                    "node_label",
                    "node_qualified_name",
                    "node_path",
                ),
            ),
            ArchitectureQuerySpec(
                name="evidence_for_symbol",
                description="Return evidence nodes for a named symbol or qualified name.",
                statement=(
                    "MATCH (n)-[:FROM_EvidencedBy]->(:EvidencedBy)-[:TO_EvidencedBy]->(e) "
                    "WHERE n.label = $name OR n.qualified_name = $name "
                    "RETURN n.id, n.label, n.qualified_name, n.path, e.id, e.label, e.path, e.line_start, e.line_end "
                    "LIMIT 100"
                ),
                parameters=("name",),
                returns=(
                    "node_id",
                    "node_label",
                    "node_qualified_name",
                    "node_path",
                    "evidence_id",
                    "evidence_label",
                    "evidence_path",
                    "evidence_line_start",
                    "evidence_line_end",
                ),
            ),
        ),
    ),
    "graph_quality_gaps": ArchitectureQueryGroup(
        name="graph_quality_gaps",
        goal="Detect graph gaps that reduce confidence in architecture claims.",
        queries=(
            ArchitectureQuerySpec(
                name="unresolved_reference_risk",
                description="Find references without resolved semantic targets.",
                statement=(
                    "MATCH (r:Reference) "
                    "WHERE NOT EXISTS { MATCH (r)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->() } "
                    "RETURN r.id, r.label, r.path, r.line_start ORDER BY r.path, r.line_start LIMIT 200"
                ),
                returns=("id", "label", "path", "line_start"),
            ),
        ),
    ),
}


def architecture_query_catalog(group: str | None = None) -> dict[str, Any]:
    """Manage query catalog within graph context and architecture-query reasoning.

    This appends structured diagnostic data when diagnostics are enabled.

    Args:
        group: Architecture-query group selected by the caller.

    Returns:
        Structured mapping that follows the graph context and architecture-query
        reasoning response contract.
    """
    groups = _selected_groups(group)
    return {
        "workflow": WORKFLOW_NAME,
        "recommended_order": list(ARCHITECTURE_QUERY_ORDER),
        "execution_tool": EXECUTION_TOOL,
        "groups": [query_group.as_dict() for query_group in groups],
    }


def _selected_groups(group: str | None) -> tuple[ArchitectureQueryGroup, ...]:
    """Manage groups within graph context and architecture-query reasoning.

    Args:
        group: Architecture-query group selected by the caller.

    Returns:
        Tuple of stable results returned to the graph context and architecture-query
        reasoning caller.

    Raises:
        ValueError: Raised when validation or runtime preconditions fail.
    """
    if group is None or group == "":
        return tuple(ARCHITECTURE_QUERY_GROUPS[name] for name in ARCHITECTURE_QUERY_ORDER)
    try:
        return (ARCHITECTURE_QUERY_GROUPS[group],)
    except KeyError as exc:
        valid = ", ".join(ARCHITECTURE_QUERY_ORDER)
        raise ValueError(f"Unknown architecture query group: {group}. Valid groups: {valid}") from exc


__all__ = [
    "ARCHITECTURE_QUERY_GROUPS",
    "ARCHITECTURE_QUERY_ORDER",
    "EXECUTION_TOOL",
    "WORKFLOW_NAME",
    "ArchitectureQueryGroup",
    "ArchitectureQuerySpec",
    "architecture_query_catalog",
]
