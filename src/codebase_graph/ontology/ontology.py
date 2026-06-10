from __future__ import annotations

from dataclasses import dataclass
from typing import Any

ONTOLOGY_NAME = "code_ontology_v1"
ONTOLOGY_VERSION = "1.0.0"


@dataclass(frozen=True, slots=True)
class FieldSpec:
    """Describe a declared field used by ontology and schema metadata."""
    name: str
    value_type: str
    description: str
    required: bool = False

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the ontology and schema metadata response contract.
        """
        return {
            "name": self.name,
            "type": self.value_type,
            "description": self.description,
            "required": self.required,
        }


@dataclass(frozen=True, slots=True)
class NodeTypeSpec:
    """Describe a declared node type used by ontology and schema metadata.

    The class belongs to Canonical ontology declarations for node types, relation types, parser
    mappings, and query helpers.
    """
    name: str
    description: str
    fields: tuple[FieldSpec, ...] = ()
    parser_node_types: tuple[str, ...] = ()
    constraints: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the ontology and schema metadata response contract.
        """
        return {
            "name": self.name,
            "description": self.description,
            "fields": [field.as_dict() for field in self.fields],
            "parser_node_types": list(self.parser_node_types),
            "constraints": list(self.constraints),
        }


@dataclass(frozen=True, slots=True)
class RelationTypeSpec:
    """Describe a declared relation type used by ontology and schema metadata.

    The class belongs to Canonical ontology declarations for node types, relation types, parser
    mappings, and query helpers.
    """
    name: str
    source_types: tuple[str, ...]
    target_types: tuple[str, ...]
    description: str
    fields: tuple[FieldSpec, ...] = ()
    constraints: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the ontology and schema metadata response contract.
        """
        return {
            "name": self.name,
            "source_types": list(self.source_types),
            "target_types": list(self.target_types),
            "description": self.description,
            "fields": [field.as_dict() for field in self.fields],
            "constraints": list(self.constraints),
        }


@dataclass(frozen=True, slots=True)
class ParserNodeMappingSpec:
    """Describe a declared parser node mapping used by ontology and schema metadata.

    The class belongs to Canonical ontology declarations for node types, relation types, parser
    mappings, and query helpers.
    """
    name: str
    parser_node_types: tuple[str, ...]
    captures: tuple[str, ...]
    target_node_types: tuple[str, ...]
    relation_types: tuple[str, ...]
    description: str
    context_rule: str = ""

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the ontology and schema metadata response contract.
        """
        return {
            "name": self.name,
            "parser_node_types": list(self.parser_node_types),
            "captures": list(self.captures),
            "target_node_types": list(self.target_node_types),
            "relation_types": list(self.relation_types),
            "description": self.description,
            "context_rule": self.context_rule,
        }


@dataclass(frozen=True, slots=True)
class QueryHelperSpec:
    """Describe a declared query helper used by ontology and schema metadata.

    The class belongs to Canonical ontology declarations for node types, relation types, parser
    mappings, and query helpers.
    """
    name: str
    description: str
    query: str
    parameters: tuple[str, ...] = ()
    returns: tuple[str, ...] = ()

    def as_dict(self) -> dict[str, Any]:
        """Serialize this object into the stable dictionary shape exposed to CLI, MCP, and tests.

        Returns:
            Structured mapping that follows the ontology and schema metadata response contract.
        """
        return {
            "name": self.name,
            "description": self.description,
            "query": self.query,
            "parameters": list(self.parameters),
            "returns": list(self.returns),
        }


COMMON_NODE_FIELDS = (
    FieldSpec("id", "string", "Stable unique node identifier.", True),
    FieldSpec("label", "string", "Short human-readable node label.", True),
    FieldSpec("kind", "string", "Ontology-specific subtype or parser-derived role."),
    FieldSpec("language", "string", "Source language when the node is code-derived."),
    FieldSpec("path", "string", "Repository-relative source path."),
    FieldSpec("qualified_name", "string", "Best-effort language-neutral qualified name."),
    FieldSpec("scope_id", "string", "Containing lexical or semantic scope id."),
    FieldSpec("line_start", "integer", "One-based start line in the source file."),
    FieldSpec("line_end", "integer", "One-based end line in the source file."),
    FieldSpec("byte_start", "integer", "Zero-based start byte in the source file."),
    FieldSpec("byte_end", "integer", "Zero-based end byte in the source file."),
    FieldSpec("tree_sitter_node_type", "string", "Raw parser node type that produced this node."),
    FieldSpec("capture_name", "string", "Tree-sitter query capture name when available."),
    FieldSpec("summary", "string", "Compact text summary used for context assembly."),
    FieldSpec("metadata", "json", "Structured extractor-specific details."),
)

EDGE_FIELDS = (
    FieldSpec("id", "string", "Stable unique relation identifier.", True),
    FieldSpec("kind", "string", "Relation subtype or evidence role."),
    FieldSpec("source_id", "string", "Source node id.", True),
    FieldSpec("target_id", "string", "Target node id.", True),
    FieldSpec("confidence", "number", "Extractor confidence between 0 and 1."),
    FieldSpec("line_start", "integer", "One-based evidence start line."),
    FieldSpec("line_end", "integer", "One-based evidence end line."),
    FieldSpec("byte_start", "integer", "Zero-based evidence start byte."),
    FieldSpec("byte_end", "integer", "Zero-based evidence end byte."),
    FieldSpec("metadata", "json", "Structured relation evidence and resolver details."),
)


def _node(
    name: str,
    description: str,
    *,
    parser_node_types: tuple[str, ...] = (),
    fields: tuple[FieldSpec, ...] = (),
    constraints: tuple[str, ...] = (),
) -> NodeTypeSpec:
    """Manage ontology and schema metadata.

    Args:
        name: Name used by the ontology and schema metadata workflow.
        description: Description used by the ontology and schema metadata workflow.
        parser_node_types: Parser node types used by the ontology and schema metadata
        workflow.
        fields: Field mapping to read or serialize.
        constraints: Constraints used by the ontology and schema metadata workflow.

    Returns:
        NodeTypeSpec instance populated with data from the ontology and schema metadata
        workflow.
    """
    return NodeTypeSpec(
        name=name,
        description=description,
        fields=COMMON_NODE_FIELDS + fields,
        parser_node_types=parser_node_types,
        constraints=constraints,
    )


NODE_TYPES = (
    _node("Repository", "A version-controlled repository or source tree boundary."),
    _node("SourceRoot", "A configured root scanned for source, docs, manifests, and generated evidence."),
    _node(
        "File",
        "A source, manifest, configuration, or documentation file.",
        fields=(
            FieldSpec("content_hash", "string", "Hash of file content at extraction time."),
            FieldSpec("size_bytes", "integer", "File size in bytes at extraction time."),
        ),
    ),
    _node(
        "Module",
        "A language-level compilation or namespace unit derived from a source file.",
        parser_node_types=("module", "program", "source_file", "Module"),
    ),
    _node(
        "ImportDeclaration",
        "An import/include/use/require declaration.",
        parser_node_types=(
            "import_statement",
            "import_from_statement",
            "import_declaration",
            "Import",
            "ImportFrom",
        ),
        fields=(FieldSpec("imported_name", "string", "Imported module, package, symbol, or path."),),
    ),
    _node(
        "ExportDeclaration",
        "An exported symbol or module boundary declaration.",
        parser_node_types=("export_statement", "export_clause", "export_declaration"),
    ),
    _node("Symbol", "A named code artifact when the exact semantic subtype is unresolved."),
    _node("Scope", "A lexical or semantic boundary for name resolution."),
    _node(
        "Class",
        "A class, struct, trait, interface, enum class, or similar type container.",
        parser_node_types=("class_definition", "class_declaration", "struct_item", "ClassDef"),
    ),
    _node(
        "Function",
        "A standalone function, lambda with stable name, or callable declaration.",
        parser_node_types=("function_definition", "function_declaration", "FunctionDef"),
    ),
    _node(
        "Method",
        "A function declared inside a class, trait, component, or object scope.",
        parser_node_types=("method_definition", "method_declaration", "FunctionDef"),
    ),
    _node("Parameter", "A callable parameter.", parser_node_types=("parameter", "typed_parameter", "arg")),
    _node("ReturnType", "A callable return type annotation.", parser_node_types=("return_type", "returns")),
    _node(
        "TypeAnnotation",
        "A type annotation attached to a symbol, parameter, assignment, or return value.",
        parser_node_types=("type", "type_identifier", "type_annotation", "annotation"),
    ),
    _node("TypeAlias", "A named alias for a type expression.", parser_node_types=("type_alias", "type_alias_declaration")),
    _node("Variable", "A mutable or local named binding.", parser_node_types=("variable_declaration", "Name")),
    _node("Constant", "A named binding treated as stable or immutable by convention or syntax."),
    _node("ClassAttribute", "A class-level attribute or static field.", parser_node_types=("AnnAssign", "field_declaration")),
    _node("InstanceAttribute", "An instance-level attribute or field assignment."),
    _node("Property", "A computed or decorated property exposed as an attribute."),
    _node("Decorator", "A decorator, annotation, macro, or attribute attached to a declaration."),
    _node("CallExpression", "A call, constructor invocation, message send, or macro invocation.", parser_node_types=("call", "Call")),
    _node("Assignment", "An assignment, binding, or destructuring declaration.", parser_node_types=("assignment", "Assign", "AnnAssign")),
    _node("Reference", "A name or member reference that may resolve to another node.", parser_node_types=("identifier", "Name")),
    _node("Literal", "A literal value from source code.", parser_node_types=("string", "integer", "float", "Constant")),
    _node("Expression", "A non-literal expression worth preserving for context or reasoning."),
    _node("ControlFlowBlock", "A branch, loop, match, switch, or guard block."),
    _node("ExceptionFlow", "A raise, throw, try, catch, except, rescue, or finally flow unit."),
    _node("APIEndpoint", "A network, RPC, CLI, event, or message endpoint exposed by code."),
    _node("Component", "A UI, service, package, or runtime component represented in source."),
    _node("Route", "A route pattern, path binding, or router entry."),
    _node("Query", "A database, search, analytics, or graph query string/expression."),
    _node("SecretRef", "A reference to a secret, credential, token, key, or sensitive environment variable."),
    _node(
        "Dependency",
        "An external package, library, framework, service, or runtime dependency.",
        fields=(
            FieldSpec("version", "string", "Declared version or version constraint."),
            FieldSpec("ecosystem", "string", "Dependency ecosystem such as pypi, npm, cargo, or go."),
        ),
    ),
    _node("DocumentationSource", "A documentation file or generated documentation artifact."),
    _node("DocumentationChunk", "A chunk or heading-level section of documentation."),
    _node(
        "SyntaxCapture",
        "Raw parser evidence preserving the concrete syntax node and capture name.",
        fields=(
            FieldSpec("sexp", "string", "Optional S-expression or compact parse-tree fragment."),
            FieldSpec("text", "string", "Optional source text captured for this syntax node."),
        ),
    ),
)


def _relation(
    name: str,
    source_types: tuple[str, ...],
    target_types: tuple[str, ...],
    description: str,
    *,
    constraints: tuple[str, ...] = (),
) -> RelationTypeSpec:
    """Return ontology and schema metadata for ontology and schema metadata.

    Args:
        name: Name used by the ontology and schema metadata workflow.
        source_types: Source types used by the ontology and schema metadata workflow.
        target_types: Target types used by the ontology and schema metadata workflow.
        description: Description used by the ontology and schema metadata workflow.
        constraints: Constraints used by the ontology and schema metadata workflow.

    Returns:
        RelationTypeSpec instance populated with data from the ontology and schema metadata
        workflow.
    """
    return RelationTypeSpec(
        name=name,
        source_types=source_types,
        target_types=target_types,
        description=description,
        fields=EDGE_FIELDS,
        constraints=constraints,
    )


DECLARATION_NODES = (
    "Symbol",
    "Class",
    "Function",
    "Method",
    "Parameter",
    "ReturnType",
    "TypeAnnotation",
    "TypeAlias",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "Decorator",
    "Assignment",
    "APIEndpoint",
    "Component",
    "Route",
    "Query",
    "SecretRef",
)

EXPRESSION_NODES = (
    "CallExpression",
    "Assignment",
    "Reference",
    "Literal",
    "Expression",
    "ControlFlowBlock",
    "ExceptionFlow",
    "Query",
    "SecretRef",
)

DOCUMENTATION_NODES = ("DocumentationSource", "DocumentationChunk")

RELATION_TYPES = (
    _relation(
        "Contains",
        ("Repository", "SourceRoot", "File", "Module", "Scope", "Class", "Function", "Method", "Component"),
        (
            "SourceRoot",
            "File",
            "Module",
            "Scope",
            "ImportDeclaration",
            "ExportDeclaration",
            *DECLARATION_NODES,
            *EXPRESSION_NODES,
            *DOCUMENTATION_NODES,
        ),
        "Structural containment between repository, files, scopes, declarations, and syntax-derived units.",
    ),
    _relation(
        "Defines",
        ("File", "Module", "Scope", "Class", "Function", "Method", "Component"),
        DECLARATION_NODES,
        "A file, module, scope, or component defines a semantic code node.",
    ),
    _relation(
        "Imports",
        ("File", "Module", "Scope"),
        ("ImportDeclaration", "Dependency", "Module", "Symbol"),
        "A source unit imports, includes, requires, or uses another unit.",
    ),
    _relation(
        "Exports",
        ("File", "Module", "Scope", "Component"),
        ("ExportDeclaration", *DECLARATION_NODES),
        "A source unit exports a declaration or public surface.",
    ),
    _relation(
        "Declares",
        ("Module", "Scope", "Class", "Function", "Method", "Assignment"),
        DECLARATION_NODES,
        "A declaration site introduces a named symbol or subordinate declaration.",
    ),
    _relation(
        "HasScope",
        ("File", "Module", *DECLARATION_NODES, *EXPRESSION_NODES),
        ("Scope",),
        "Connects a node to the lexical or semantic scope used for resolution.",
    ),
    _relation(
        "HasParameter",
        ("Function", "Method", "APIEndpoint", "Route", "CallExpression"),
        ("Parameter",),
        "Connects callables, endpoints, routes, or calls to their parameters or arguments.",
    ),
    _relation(
        "HasReturnType",
        ("Function", "Method", "APIEndpoint"),
        ("ReturnType",),
        "Connects callables or endpoints to their return type node.",
    ),
    _relation(
        "HasTypeAnnotation",
        ("Symbol", "Parameter", "ReturnType", "TypeAlias", "Variable", "Constant", "ClassAttribute", "InstanceAttribute"),
        ("TypeAnnotation", "Reference", "Literal"),
        "Connects a typed code node to its annotation expression.",
    ),
    _relation(
        "Assigns",
        ("Assignment", "Variable", "Constant", "ClassAttribute", "InstanceAttribute", "Property"),
        ("Variable", "Constant", "ClassAttribute", "InstanceAttribute", "Property", "Literal", "Expression", "CallExpression"),
        "Connects an assignment site or assigned symbol to the target or assigned value.",
    ),
    _relation(
        "References",
        (
            "Reference",
            "Expression",
            "CallExpression",
            "Assignment",
            "ControlFlowBlock",
            "TypeAnnotation",
            "Decorator",
            "Query",
            "SecretRef",
        ),
        ("Symbol", "Class", "Function", "Method", "Variable", "Constant", "ClassAttribute", "InstanceAttribute", "Property", "Parameter", "Module", "Dependency"),
        "A source reference mentions another semantic node without necessarily resolving to it.",
    ),
    _relation(
        "Calls",
        ("Function", "Method", "CallExpression", "Decorator", "APIEndpoint", "Route", "Component"),
        ("CallExpression", "Function", "Method", "Class", "APIEndpoint"),
        "A callable or call expression invokes another callable-like target.",
    ),
    _relation(
        "DecoratedBy",
        ("Class", "Function", "Method", "Property", "APIEndpoint", "Route", "Component"),
        ("Decorator", "CallExpression", "Reference"),
        "A declaration is modified by a decorator, annotation, macro, or framework marker.",
    ),
    _relation(
        "ResolvesTo",
        ("Reference", "ImportDeclaration", "CallExpression", "TypeAnnotation", "Decorator"),
        ("Symbol", "Module", "Class", "Function", "Method", "Variable", "Constant", "Dependency", "Parameter"),
        "A resolver maps a syntactic reference to the best semantic target.",
    ),
    _relation(
        "DependsOn",
        ("Repository", "SourceRoot", "File", "Module", "ImportDeclaration", "Dependency", "Component"),
        ("Dependency", "Module", "Component", "SecretRef"),
        "A repository or code unit depends on an external or internal dependency.",
    ),
    _relation(
        "Documents",
        ("DocumentationSource", "DocumentationChunk", "Literal"),
        ("Repository", "File", "Module", *DECLARATION_NODES),
        "Documentation describes a repository, source unit, or semantic declaration.",
    ),
    _relation(
        "RoutesTo",
        ("Route", "APIEndpoint", "Component"),
        ("APIEndpoint", "Function", "Method", "Component"),
        "A route or component dispatches to an endpoint or handler.",
    ),
    _relation(
        "Exposes",
        ("Repository", "Module", "Component", "APIEndpoint", "Route"),
        ("APIEndpoint", "Route", "Function", "Method", "Component", "ExportDeclaration"),
        "A source unit exposes a public runtime or module surface.",
    ),
    _relation(
        "ExecutesQuery",
        ("Function", "Method", "CallExpression", "APIEndpoint", "Component"),
        ("Query",),
        "A code path executes or constructs a query.",
    ),
    _relation(
        "UsesSecret",
        ("Function", "Method", "CallExpression", "Component", "APIEndpoint", "Dependency"),
        ("SecretRef",),
        "A code path or dependency uses a secret or sensitive configuration value.",
    ),
    _relation(
        "Raises",
        ("Function", "Method", "CallExpression", "ControlFlowBlock"),
        ("ExceptionFlow",),
        "A code path raises or throws an exception flow.",
    ),
    _relation(
        "Handles",
        ("Function", "Method", "ControlFlowBlock", "ExceptionFlow"),
        ("ExceptionFlow",),
        "A code path handles or catches an exception flow.",
    ),
    _relation(
        "DerivedFrom",
        (*DECLARATION_NODES, *EXPRESSION_NODES, *DOCUMENTATION_NODES, "Module", "ImportDeclaration", "ExportDeclaration"),
        ("SyntaxCapture",),
        "A semantic node was derived from a raw parser capture.",
    ),
    _relation(
        "EvidencedBy",
        ("Repository", "File", "Module", *DECLARATION_NODES, *EXPRESSION_NODES, "Dependency", *DOCUMENTATION_NODES),
        ("SyntaxCapture", "File", "DocumentationChunk"),
        "A semantic claim is supported by parser, file, or documentation evidence.",
    ),
)

PARSER_NODE_MAPPINGS = (
    ParserNodeMappingSpec(
        "module",
        ("module", "program", "source_file", "Module"),
        ("module", "source_file"),
        ("Module",),
        ("Contains", "Defines", "DerivedFrom"),
        "Create one Module node per parser root or language namespace root.",
    ),
    ParserNodeMappingSpec(
        "imports",
        ("import_statement", "import_from_statement", "import_declaration", "Import", "ImportFrom"),
        ("import", "reference.import", "reference.include", "reference.require", "reference.use"),
        ("ImportDeclaration",),
        ("Imports", "DependsOn", "DerivedFrom"),
        "Normalize import-like declarations across languages and attach imported names as metadata.",
    ),
    ParserNodeMappingSpec(
        "exports",
        ("export_statement", "export_clause", "export_declaration"),
        ("export", "definition.export"),
        ("ExportDeclaration",),
        ("Exports", "DerivedFrom"),
        "Capture public export declarations and declarations marked as exported.",
    ),
    ParserNodeMappingSpec(
        "classes",
        ("class_definition", "class_declaration", "struct_item", "interface_declaration", "ClassDef"),
        ("definition.class", "definition.struct", "definition.interface"),
        ("Class",),
        ("Defines", "Declares", "HasScope", "DecoratedBy", "DerivedFrom"),
        "Map class-like containers to Class nodes with a child Scope.",
    ),
    ParserNodeMappingSpec(
        "functions_and_methods",
        ("function_definition", "function_declaration", "method_definition", "method_declaration", "FunctionDef"),
        ("definition.function", "definition.method"),
        ("Function", "Method"),
        ("Defines", "Declares", "HasScope", "HasParameter", "HasReturnType", "DecoratedBy", "DerivedFrom"),
        "Create Function for module-level callables and Method when the callable is enclosed by Class or Component.",
        context_rule="enclosing Class or Component changes the target node from Function to Method",
    ),
    ParserNodeMappingSpec(
        "parameters",
        ("parameter", "typed_parameter", "default_parameter", "arg"),
        ("definition.parameter", "parameter"),
        ("Parameter",),
        ("HasParameter", "HasTypeAnnotation", "DerivedFrom"),
        "Create Parameter nodes for callable parameter declarations.",
    ),
    ParserNodeMappingSpec(
        "return_types",
        ("return_type", "type", "type_identifier", "returns"),
        ("type.return", "return_type"),
        ("ReturnType",),
        ("HasReturnType", "HasTypeAnnotation", "References", "DerivedFrom"),
        "Capture explicit return type annotations.",
    ),
    ParserNodeMappingSpec(
        "type_annotations",
        ("type", "type_identifier", "type_annotation", "annotation", "Name"),
        ("type", "type.annotation", "reference.type"),
        ("TypeAnnotation",),
        ("HasTypeAnnotation", "References", "ResolvesTo", "DerivedFrom"),
        "Capture type annotation expressions attached to declarations.",
    ),
    ParserNodeMappingSpec(
        "type_aliases",
        ("type_alias", "type_alias_declaration"),
        ("definition.type_alias",),
        ("TypeAlias",),
        ("Defines", "HasTypeAnnotation", "DerivedFrom"),
        "Capture named type aliases.",
    ),
    ParserNodeMappingSpec(
        "assignments",
        ("assignment", "assignment_expression", "variable_declaration", "Assign", "AnnAssign"),
        ("definition.variable", "definition.constant", "assignment"),
        ("Assignment", "Variable", "Constant", "ClassAttribute", "InstanceAttribute", "Property"),
        ("Defines", "Declares", "Assigns", "HasTypeAnnotation", "DerivedFrom"),
        "Normalize assignments; scope, naming convention, and receiver decide variable, constant, or attribute node type.",
    ),
    ParserNodeMappingSpec(
        "decorators",
        ("decorator", "attribute_item", "annotation", "Call", "Name"),
        ("decorator", "definition.decorator"),
        ("Decorator",),
        ("DecoratedBy", "Calls", "References", "DerivedFrom"),
        "Capture decorators, annotations, macros, or framework markers that modify declarations.",
    ),
    ParserNodeMappingSpec(
        "calls",
        ("call", "call_expression", "invocation_expression", "Call"),
        ("reference.call", "call"),
        ("CallExpression",),
        ("Calls", "References", "ResolvesTo", "DerivedFrom"),
        "Create call-expression nodes and optionally resolve them to callable targets.",
    ),
    ParserNodeMappingSpec(
        "references",
        ("identifier", "field_identifier", "attribute", "Name", "Attribute"),
        ("reference", "reference.identifier", "reference.member"),
        ("Reference",),
        ("References", "ResolvesTo", "DerivedFrom"),
        "Capture name and member references before or after semantic resolution.",
    ),
    ParserNodeMappingSpec(
        "literals",
        ("string", "integer", "float", "true", "false", "null", "none", "Constant"),
        ("literal", "string", "number"),
        ("Literal",),
        ("Contains", "References", "DerivedFrom"),
        "Capture literals that are useful for docs, routes, queries, secrets, or assignment values.",
    ),
    ParserNodeMappingSpec(
        "control_flow",
        ("if_statement", "for_statement", "while_statement", "match_statement", "switch_statement"),
        ("control_flow",),
        ("ControlFlowBlock",),
        ("Contains", "References", "DerivedFrom"),
        "Capture branch and loop blocks when they affect reasoning or dependency paths.",
    ),
    ParserNodeMappingSpec(
        "exception_flow",
        ("try_statement", "except_clause", "catch_clause", "raise_statement", "throw_statement"),
        ("exception", "raises", "handles"),
        ("ExceptionFlow",),
        ("Raises", "Handles", "DerivedFrom"),
        "Capture exception raising and handling paths.",
    ),
    ParserNodeMappingSpec(
        "routes_and_endpoints",
        ("decorator", "call", "route_declaration", "handler_definition"),
        ("entrypoint.api", "route", "endpoint"),
        ("APIEndpoint", "Route"),
        ("RoutesTo", "Exposes", "DecoratedBy", "DerivedFrom"),
        "Create APIEndpoint and Route nodes from framework route declarations or decorated handlers.",
    ),
    ParserNodeMappingSpec(
        "components",
        ("class_definition", "function_definition", "jsx_element", "component_declaration"),
        ("definition.component", "component"),
        ("Component",),
        ("Defines", "Contains", "Exposes", "DerivedFrom"),
        "Capture UI, service, runtime, or package components when extractor rules identify them.",
    ),
    ParserNodeMappingSpec(
        "queries",
        ("string", "template_string", "call", "Call"),
        ("query.sql", "query.graph", "query.search"),
        ("Query",),
        ("ExecutesQuery", "References", "DerivedFrom"),
        "Capture query strings or query builder expressions.",
    ),
    ParserNodeMappingSpec(
        "secrets",
        ("identifier", "string", "attribute", "Name", "Constant"),
        ("secret", "secret.env", "secret.ref"),
        ("SecretRef",),
        ("UsesSecret", "References", "DerivedFrom"),
        "Capture secret-looking names, environment references, keys, and credential handles.",
    ),
    ParserNodeMappingSpec(
        "documentation",
        ("comment", "string", "docstring", "DocumentationSource", "DocumentationChunk"),
        ("doc", "doc.string", "doc.comment"),
        ("DocumentationSource", "DocumentationChunk"),
        ("Documents", "EvidencedBy"),
        "Capture documentation sources and chunks from docs, comments, and docstrings.",
    ),
)

SEARCH_INDEXES = (
    {"name": "idx_code_symbols", "node_types": ["Symbol", "Class", "Function", "Method", "Variable", "Constant"], "fields": ["label", "qualified_name", "summary"]},
    {"name": "idx_source_units", "node_types": ["Repository", "SourceRoot", "File", "Module"], "fields": ["label", "path", "summary"]},
    {"name": "idx_dependencies", "node_types": ["ImportDeclaration", "Dependency"], "fields": ["label", "qualified_name", "summary"]},
    {"name": "idx_runtime_surface", "node_types": ["APIEndpoint", "Component", "Route", "Query", "SecretRef"], "fields": ["label", "qualified_name", "summary"]},
    {"name": "idx_docs", "node_types": ["DocumentationSource", "DocumentationChunk"], "fields": ["label", "path", "summary"]},
)

CONTEXT_PROFILES = {
    "brief": {
        "description": "Smallest useful context: matched nodes plus direct defining file/module.",
        "relations": ["Contains", "Defines", "EvidencedBy"],
        "max_depth": 1,
    },
    "definitions": {
        "description": "Definition-oriented context for symbols and scopes.",
        "relations": ["Defines", "Declares", "HasScope", "HasParameter", "HasReturnType", "HasTypeAnnotation"],
        "max_depth": 2,
    },
    "dependencies": {
        "description": "Import and dependency context.",
        "relations": ["Imports", "DependsOn", "References", "ResolvesTo"],
        "max_depth": 2,
    },
    "callgraph": {
        "description": "Callable neighborhood for callers, callees, and call expressions.",
        "relations": ["Calls", "References", "ResolvesTo"],
        "max_depth": 2,
    },
    "runtime": {
        "description": "Runtime surface context for routes, endpoints, queries, and secrets.",
        "relations": ["RoutesTo", "Exposes", "ExecutesQuery", "UsesSecret"],
        "max_depth": 2,
    },
    "docs": {
        "description": "Documentation context connected to code artifacts.",
        "relations": ["Documents", "EvidencedBy"],
        "max_depth": 1,
    },
    "change_impact": {
        "description": "Context for likely downstream impact of changing a symbol.",
        "relations": ["Defines", "References", "Calls", "RoutesTo", "ExecutesQuery", "UsesSecret", "DependsOn"],
        "max_depth": 3,
    },
}

SYMBOL_LOOKUP_DEFINITION_TYPES = (
    "Class",
    "Function",
    "Method",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "Parameter",
    "TypeAlias",
)

SYMBOL_LOOKUP_QUERY = (
    " UNION ALL ".join(
        f"MATCH (s:{node_type}) "
        "WHERE s.label = $name OR s.qualified_name = $name "
        "RETURN s.id, s.label, s.qualified_name, s.path"
        for node_type in SYMBOL_LOOKUP_DEFINITION_TYPES
    )
    + " LIMIT 25"
)

QUERY_HELPERS = (
    QueryHelperSpec(
        "repository_overview",
        "List high-level source roots, files, modules, dependencies, and runtime surfaces.",
        "MATCH (n) WHERE n:SourceRoot OR n:File OR n:Module OR n:Dependency OR n:APIEndpoint OR n:Component RETURN n.id, n.label, n.path LIMIT 100",
        returns=("id", "label", "path"),
    ),
    QueryHelperSpec(
        "symbol_lookup",
        "Find concrete semantic definitions by label or qualified name.",
        SYMBOL_LOOKUP_QUERY,
        parameters=("name",),
        returns=("id", "label", "qualified_name", "path"),
    ),
    QueryHelperSpec(
        "definition_context",
        "Find a named class, function, method, variable, or constant definition.",
        "MATCH (d) WHERE d:Class OR d:Function OR d:Method OR d:Variable OR d:Constant RETURN d.id, d.label, d.kind, d.path LIMIT 50",
        returns=("id", "label", "kind", "path"),
    ),
    QueryHelperSpec(
        "callgraph_neighborhood",
        "Find call expressions and resolved callable targets near a symbol.",
        "MATCH (c:CallExpression)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->(target) RETURN c.id, c.path, target.id, target.label LIMIT 50",
        returns=("call_id", "path", "target_id", "target_label"),
    ),
    QueryHelperSpec(
        "dependency_map",
        "Inspect imports and dependencies.",
        "MATCH (i:ImportDeclaration)-[:FROM_DependsOn]->(:DependsOn)-[:TO_DependsOn]->(d:Dependency) RETURN i.id, i.label, d.id, d.label LIMIT 100",
        returns=("import_id", "import_label", "dependency_id", "dependency_label"),
    ),
    QueryHelperSpec(
        "runtime_surface",
        "Inspect routes, endpoints, executed queries, and secret use.",
        "MATCH (r:Route)-[:FROM_RoutesTo]->(:RoutesTo)-[:TO_RoutesTo]->(e:APIEndpoint) RETURN r.id, r.label, e.id, e.label LIMIT 100",
        returns=("route_id", "route_label", "endpoint_id", "endpoint_label"),
    ),
    QueryHelperSpec(
        "documentation_context",
        "Find documentation chunks connected to code nodes.",
        "MATCH (d:DocumentationChunk)-[:FROM_Documents]->(:Documents)-[:TO_Documents]->(n) RETURN d.id, d.label, n.id, n.label LIMIT 50",
        returns=("doc_id", "doc_label", "node_id", "node_label"),
    ),
    QueryHelperSpec(
        "unresolved_references",
        "Find references that have not been resolved to a semantic target.",
        "MATCH (r:Reference) "
        "WHERE NOT EXISTS { MATCH (r)-[:FROM_ResolvesTo]->(:ResolvesTo)-[:TO_ResolvesTo]->() } "
        "RETURN r.id, r.label, r.path, r.line_start LIMIT 100",
        returns=("id", "label", "path", "line_start"),
    ),
)


def node_type_names() -> tuple[str, ...]:
    """Manage type names within ontology and schema metadata.

    Returns:
        Tuple of stable results returned to the ontology and schema metadata caller.
    """
    return tuple(node.name for node in NODE_TYPES)


def relation_type_names() -> tuple[str, ...]:
    """Return type names for ontology and schema metadata.

    Returns:
        Tuple of stable results returned to the ontology and schema metadata caller.
    """
    return tuple(relation.name for relation in RELATION_TYPES)


def get_node_type(name: str) -> NodeTypeSpec:
    """Return node type for ontology and schema metadata.

    Args:
        name: Name used by the ontology and schema metadata workflow.

    Returns:
        NodeTypeSpec instance populated with data from the ontology and schema metadata
        workflow.

    Raises:
        KeyError: Raised when validation or runtime preconditions fail.
    """
    for node_type in NODE_TYPES:
        if node_type.name == name:
            return node_type
    raise KeyError(name)


def get_relation_type(name: str) -> RelationTypeSpec:
    """Return relation type for ontology and schema metadata.

    Args:
        name: Name used by the ontology and schema metadata workflow.

    Returns:
        RelationTypeSpec instance populated with data from the ontology and schema metadata
        workflow.

    Raises:
        KeyError: Raised when validation or runtime preconditions fail.
    """
    for relation_type in RELATION_TYPES:
        if relation_type.name == name:
            return relation_type
    raise KeyError(name)


def schema_payload() -> dict[str, Any]:
    """Build payload for ontology and schema metadata.

    Returns:
        Structured mapping that follows the ontology and schema metadata response contract.
    """
    return {
        "ontology": ONTOLOGY_NAME,
        "version": ONTOLOGY_VERSION,
        "node_types": [node.as_dict() for node in NODE_TYPES],
        "relation_types": [relation.as_dict() for relation in RELATION_TYPES],
        "parser_node_mappings": [mapping.as_dict() for mapping in PARSER_NODE_MAPPINGS],
        "search_indexes": list(SEARCH_INDEXES),
        "context_profiles": CONTEXT_PROFILES,
        "query_helpers": [helper.as_dict() for helper in QUERY_HELPERS],
    }
