use crate::normalize::SyntaxNode;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Capture {
    capture_name: String,
    node_type: String,
    label: String,
    text: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    fields: Vec<String>,
}

#[derive(Clone)]
struct Node {
    id: String,
    table: String,
    label: String,
    kind: String,
    language: String,
    path: String,
    qualified_name: String,
    scope_id: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    tree_sitter_node_type: String,
    capture_name: String,
    summary: String,
    metadata: BTreeMap<String, JsonValue>,
}

#[derive(Clone)]
struct Edge {
    id: String,
    edge_type: String,
    source_id: String,
    target_id: String,
    kind: String,
    metadata: BTreeMap<String, JsonValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphNodeRow {
    pub(crate) id: String,
    pub(crate) table: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) language: String,
    pub(crate) path: String,
    pub(crate) qualified_name: String,
    pub(crate) scope_id: String,
    pub(crate) line_start: Option<i64>,
    pub(crate) line_end: Option<i64>,
    pub(crate) byte_start: Option<i64>,
    pub(crate) byte_end: Option<i64>,
    pub(crate) tree_sitter_node_type: String,
    pub(crate) capture_name: String,
    pub(crate) summary: String,
    pub(crate) metadata: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphEdgeRow {
    pub(crate) id: String,
    pub(crate) edge_type: String,
    pub(crate) source_id: String,
    pub(crate) target_id: String,
    pub(crate) kind: String,
    pub(crate) metadata: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuiltGraphRows {
    pub(crate) nodes: Vec<GraphNodeRow>,
    pub(crate) edges: Vec<GraphEdgeRow>,
}

#[derive(Clone)]
enum JsonValue {
    String(String),
    Array(Vec<JsonValue>),
}

struct Builder {
    path: String,
    language: String,
    source_root: String,
    repository_label: String,
    nodes: BTreeMap<String, Node>,
    edges: BTreeMap<String, Edge>,
    symbols_by_name: HashMap<String, Vec<String>>,
    relation_allowlist: RelationAllowlist,
}

#[derive(Clone, Default)]
struct RelationAllowlist {
    enabled: bool,
    pairs_by_relation: BTreeMap<String, BTreeSet<(String, String)>>,
}

#[derive(Deserialize)]
struct RelationSpecPayload {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source_types: Vec<String>,
    #[serde(default)]
    target_types: Vec<String>,
}

impl RelationAllowlist {
    fn from_meta(meta: &BTreeMap<String, String>) -> Result<Self, String> {
        let Some(encoded) = meta.get("ontology_relations") else {
            return Ok(Self::default());
        };
        let relation_specs: Vec<RelationSpecPayload> = serde_json::from_str(encoded)
            .map_err(|error| format!("invalid ontology_relations metadata: {error}"))?;
        let mut pairs_by_relation: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
        for relation in relation_specs {
            if relation.name.is_empty() {
                continue;
            }
            let pairs = pairs_by_relation.entry(relation.name).or_default();
            for source in &relation.source_types {
                for target in &relation.target_types {
                    pairs.insert((source.clone(), target.clone()));
                }
            }
        }
        Ok(Self {
            enabled: true,
            pairs_by_relation,
        })
    }

    fn allows(&self, edge_type: &str, source: &str, target: &str) -> bool {
        if !self.enabled {
            return legacy_relation_allowed(edge_type, source, target);
        }
        self.pairs_by_relation
            .get(edge_type)
            .is_some_and(|pairs| pairs.contains(&(source.to_string(), target.to_string())))
    }
}

struct Owner {
    node_id: String,
    table: String,
    qualified_name: String,
    scope_id: String,
}

pub fn run_cli() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    if first_record_kind(&input).as_deref() == Some("BULK") {
        return run_bulk_staging(&input);
    }
    if first_record_kind(&input).as_deref() == Some("TSNORM") {
        return run_tree_sitter_normalization(&input);
    }
    if first_record_kind(&input).as_deref() == Some("SCAN") {
        return run_scan_diff(&input);
    }
    if first_record_kind(&input).as_deref() == Some("SEMANTIC") {
        return run_semantic_batch(&input);
    }
    let (meta, captures) = parse_input(&input)?;
    let mut builder = Builder::new(meta)?;
    builder.build(captures)?;
    print!("{}", builder.encode_output());
    Ok(())
}

#[allow(dead_code)]
pub(crate) struct LegacyBulkStagingOutput {
    pub(crate) copy_statements: Vec<String>,
    pub(crate) node_rows: usize,
    pub(crate) edge_rows: usize,
    pub(crate) connector_rows: usize,
}

pub fn build_graph_output(input: &str) -> Result<String, String> {
    let (meta, captures) = parse_input(input)?;
    let mut builder = Builder::new(meta)?;
    builder.build(captures)?;
    Ok(builder.encode_output())
}

#[allow(dead_code)]
pub(crate) fn build_tree_graph_output(input: &str) -> Result<String, String> {
    let parsed = parse_tree_graph_input(input)?;
    let mut builder = Builder::new(parsed.meta)?;
    builder.build_tree(&parsed.nodes, parsed.root_id)?;
    Ok(builder.encode_output())
}

pub(crate) fn build_syntax_tree_graph_rows(
    meta: BTreeMap<String, String>,
    root: &SyntaxNode,
) -> Result<BuiltGraphRows, String> {
    let mut builder = Builder::new(meta)?;
    let mut nodes = BTreeMap::new();
    let root_id = append_syntax_tree_node(root, None, &mut 0, &mut nodes);
    builder.build_tree(&nodes, root_id)?;
    Ok(builder.typed_rows())
}

#[allow(dead_code)]
pub(crate) fn write_bulk_staging_output(input: &str) -> Result<LegacyBulkStagingOutput, String> {
    let output = parse_bulk_staging(input)?.write()?;
    Ok(LegacyBulkStagingOutput {
        copy_statements: output.copy_statements,
        node_rows: output.node_rows,
        edge_rows: output.edge_rows,
        connector_rows: output.connector_rows,
    })
}

fn first_record_kind(input: &str) -> Option<String> {
    input
        .lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| line.split('\t').next())
        .map(str::to_string)
}

impl Builder {
    fn new(meta: BTreeMap<String, String>) -> Result<Self, String> {
        let relation_allowlist = RelationAllowlist::from_meta(&meta)?;
        Ok(Self {
            path: meta.get("path").cloned().unwrap_or_default(),
            language: meta.get("language").cloned().unwrap_or_default(),
            source_root: meta.get("source_root").cloned().unwrap_or_default(),
            repository_label: meta
                .get("repository_label")
                .cloned()
                .unwrap_or_else(|| "repository".to_string()),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            symbols_by_name: HashMap::new(),
            relation_allowlist,
        })
    }

    fn build(&mut self, captures: Vec<Capture>) -> Result<(), String> {
        let repository_label = self.repository_label.clone();
        let source_root = self.source_root.clone();
        let path = self.path.clone();
        let repository = self.support_node("Repository", &repository_label, &repository_label, "");
        let source = self.support_node("SourceRoot", &source_root, &source_root, &source_root);
        let file_label = self
            .path
            .rsplit('/')
            .next()
            .unwrap_or(self.path.as_str())
            .to_string();
        let file = self.support_node("File", &path, &file_label, &path);
        self.edge(
            "Contains",
            &repository.id,
            &source.id,
            "repository_source_root",
            BTreeMap::new(),
        )?;
        self.edge(
            "Contains",
            &source.id,
            &file.id,
            "source_root_file",
            BTreeMap::new(),
        )?;

        let module_capture = Capture {
            capture_name: String::new(),
            node_type: "Module".to_string(),
            label: "Module".to_string(),
            text: String::new(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            fields: Vec::new(),
        };
        self.syntax_capture(&module_capture);
        let module = self.semantic_node(
            "Module",
            &module_capture,
            &module_label(&self.path),
            &file.id,
            "",
            None,
        );
        let module_scope = self.scope_for(&module);
        self.edge(
            "Contains",
            &file.id,
            &module.id,
            "file_module",
            BTreeMap::new(),
        )?;
        self.edge(
            "Contains",
            &module.id,
            &module_scope.id,
            "module_contains_scope",
            BTreeMap::new(),
        )?;
        self.edge(
            "HasScope",
            &module.id,
            &module_scope.id,
            "module_scope",
            BTreeMap::new(),
        )?;

        let owner = Owner {
            node_id: module.id.clone(),
            table: "Module".to_string(),
            qualified_name: module.qualified_name.clone(),
            scope_id: module_scope.id.clone(),
        };
        for capture in captures {
            self.emit_capture(&capture, &owner)?;
        }
        Ok(())
    }

    fn build_tree(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        root_id: usize,
    ) -> Result<(), String> {
        let Some(root) = nodes.get(&root_id) else {
            return Err("tree graph root node is missing".to_string());
        };
        let repository_label = self.repository_label.clone();
        let source_root = self.source_root.clone();
        let path = self.path.clone();
        let repository = self.support_node("Repository", &repository_label, &repository_label, "");
        let source = self.support_node("SourceRoot", &source_root, &source_root, &source_root);
        let file_label = self
            .path
            .rsplit('/')
            .next()
            .unwrap_or(self.path.as_str())
            .to_string();
        let file = self.support_node("File", &path, &file_label, &path);
        self.edge(
            "Contains",
            &repository.id,
            &source.id,
            "repository_source_root",
            BTreeMap::new(),
        )?;
        self.edge(
            "Contains",
            &source.id,
            &file.id,
            "source_root_file",
            BTreeMap::new(),
        )?;

        if matches!(
            root.node_type.as_str(),
            "Module" | "module" | "program" | "source_file"
        ) {
            let root_capture = tree_capture(root);
            let syntax_id = self.syntax_capture(&root_capture);
            let module = self.semantic_node(
                "Module",
                &root_capture,
                &module_label(&self.path),
                &file.id,
                "",
                None,
            );
            let module_scope = self.scope_for(&module);
            self.edge(
                "Contains",
                &file.id,
                &module.id,
                "file_module",
                BTreeMap::new(),
            )?;
            self.edge(
                "Contains",
                &module.id,
                &module_scope.id,
                "module_contains_scope",
                BTreeMap::new(),
            )?;
            self.edge(
                "HasScope",
                &module.id,
                &module_scope.id,
                "module_scope",
                BTreeMap::new(),
            )?;
            if should_derive_root_module(&self.language, &root.node_type) {
                self.derived_from(&module.id, &syntax_id)?;
            }
            let owner = Owner {
                node_id: module.id.clone(),
                table: "Module".to_string(),
                qualified_name: module.qualified_name.clone(),
                scope_id: module_scope.id.clone(),
            };
            for child_id in &root.children {
                self.traverse_tree_node(nodes, *child_id, &owner)?;
            }
        } else {
            let file_scope = self.scope_for(&file);
            self.edge(
                "HasScope",
                &file.id,
                &file_scope.id,
                "file_scope",
                BTreeMap::new(),
            )?;
            let owner = Owner {
                node_id: file.id.clone(),
                table: "File".to_string(),
                qualified_name: file.qualified_name.clone(),
                scope_id: file_scope.id.clone(),
            };
            self.traverse_tree_node(nodes, root_id, &owner)?;
        }
        Ok(())
    }

    fn traverse_tree_node(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        node_id: usize,
        owner: &Owner,
    ) -> Result<(), String> {
        let Some(node) = nodes.get(&node_id) else {
            return Err(format!("tree graph node {node_id} is missing"));
        };
        let next_owner = self.emit_tree_node(nodes, node, owner)?;
        let child_owner = next_owner.as_ref().unwrap_or(owner);
        for child_id in semantic_child_ids(nodes, node, &self.language) {
            self.traverse_tree_node(nodes, child_id, child_owner)?;
        }
        self.emit_parser_like_metadata_fields(node, child_owner)?;
        Ok(())
    }

    fn emit_tree_node(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        node: &TsNode,
        owner: &Owner,
    ) -> Result<Option<Owner>, String> {
        let mut capture = tree_capture(node);
        if self.language == "python" {
            capture.capture_name.clear();
        }
        let syntax_id = self.syntax_capture(&capture);
        let capture_table = table_for_capture(&capture.capture_name, owner);
        let from_capture = capture_table.is_some();
        let Some(table) = capture_table.or_else(|| table_for_node_type(&capture.node_type, owner))
        else {
            return Ok(None);
        };
        let semantic = match table.as_str() {
            "ImportDeclaration" => self.emit_tree_import(node, owner, &syntax_id)?,
            "Class" | "Function" | "Method" => {
                self.emit_declaration(&table, &capture, owner, &syntax_id)?
            }
            "Assignment" => self.emit_tree_assignment(nodes, node, owner, &syntax_id)?,
            "CallExpression" => self.emit_call(&capture, owner, &syntax_id)?,
            "Reference" => self.emit_reference(&capture, owner, &syntax_id)?,
            "Literal" => {
                let literal_capture = if self.language == "fortran" {
                    fortran_literal_capture(nodes, node).unwrap_or_else(|| capture.clone())
                } else {
                    capture.clone()
                };
                self.emit_simple_semantic("Literal", &literal_capture, owner, &syntax_id)?
            }
            _ => self.emit_simple_semantic(&table, &capture, owner, &syntax_id)?,
        };
        if matches!(
            table.as_str(),
            "Class" | "Function" | "Method" | "Component"
        ) {
            let scope = self.scope_for(&semantic);
            self.edge(
                "Contains",
                &semantic.id,
                &scope.id,
                &format!("{}_contains_scope", table.to_lowercase()),
                BTreeMap::new(),
            )?;
            self.edge(
                "HasScope",
                &semantic.id,
                &scope.id,
                &format!("{}_scope", table.to_lowercase()),
                BTreeMap::new(),
            )?;
            if matches!(table.as_str(), "Function" | "Method")
                && (self.language == "python" || !from_capture)
            {
                self.emit_tree_parameters(nodes, node, &semantic)?;
                self.emit_tree_return_type(nodes, node, &semantic)?;
            }
            return Ok(Some(Owner {
                node_id: semantic.id.clone(),
                table,
                qualified_name: semantic.qualified_name.clone(),
                scope_id: scope.id.clone(),
            }));
        }
        Ok(None)
    }

    fn emit_capture(&mut self, capture: &Capture, owner: &Owner) -> Result<Option<Owner>, String> {
        let syntax_id = self.syntax_capture(capture);
        let Some(table) = table_for_capture(&capture.capture_name, owner) else {
            return Ok(None);
        };
        let semantic = match table.as_str() {
            "ImportDeclaration" => self.emit_import(capture, owner, &syntax_id)?,
            "Class" | "Function" | "Method" => {
                self.emit_declaration(&table, capture, owner, &syntax_id)?
            }
            "CallExpression" => self.emit_call(capture, owner, &syntax_id)?,
            "Reference" => self.emit_reference(capture, owner, &syntax_id)?,
            _ => self.emit_simple_semantic(&table, capture, owner, &syntax_id)?,
        };
        if matches!(
            table.as_str(),
            "Class" | "Function" | "Method" | "Component"
        ) {
            let scope = self.scope_for(&semantic);
            self.edge(
                "Contains",
                &semantic.id,
                &scope.id,
                &format!("{}_contains_scope", table.to_lowercase()),
                BTreeMap::new(),
            )?;
            self.edge(
                "HasScope",
                &semantic.id,
                &scope.id,
                &format!("{}_scope", table.to_lowercase()),
                BTreeMap::new(),
            )?;
            return Ok(Some(Owner {
                node_id: semantic.id.clone(),
                table,
                qualified_name: semantic.qualified_name.clone(),
                scope_id: scope.id.clone(),
            }));
        }
        Ok(None)
    }

    fn emit_tree_import(
        &mut self,
        node: &TsNode,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let mut capture = tree_capture(node);
        if self.language == "python" {
            capture.capture_name.clear();
        }
        if let Some(label) = import_label(node) {
            capture.label = label;
        }
        self.emit_import(&capture, owner, syntax_id)
    }

    fn emit_tree_assignment(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        node: &TsNode,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let capture = tree_capture(node);
        let label = if self.language == "python" {
            "Assignment"
        } else {
            &capture.label
        };
        let assignment = self.semantic_node(
            "Assignment",
            &capture,
            label,
            &owner.node_id,
            &owner.qualified_name,
            None,
        );
        self.connect_owner(owner, &assignment)?;
        self.derived_from(&assignment.id, syntax_id)?;

        if let Some(target_label) = assignment_target_label(nodes, node) {
            let target_table = assignment_target_table(&target_label, owner, &capture.node_type);
            let target = self.semantic_node(
                target_table,
                &capture,
                &target_label,
                &owner.node_id,
                &owner.qualified_name,
                None,
            );
            self.connect_owner(owner, &target)?;
            self.edge_if_allowed(
                "Defines",
                &owner.node_id,
                &target.id,
                &format!("defines_{}", target_table.to_lowercase()),
                BTreeMap::new(),
            )?;
            self.edge_if_allowed(
                "Assigns",
                &assignment.id,
                &target.id,
                "assignment_target",
                BTreeMap::new(),
            )?;
            self.derived_from(&target.id, syntax_id)?;
            if let Some(annotation) = json_field_label(node, "annotation") {
                let type_node = self.emit_type_annotation_label(
                    &annotation,
                    &capture,
                    &target.id,
                    &target.qualified_name,
                )?;
                self.edge_if_allowed(
                    "HasTypeAnnotation",
                    &target.id,
                    &type_node.id,
                    "assignment_annotation",
                    BTreeMap::new(),
                )?;
            }
        }

        if let Some(value_id) = call_value_child(nodes, node) {
            let Some(value_node) = nodes.get(&value_id) else {
                return Ok(assignment);
            };
            let call_capture = tree_capture(value_node);
            let call_syntax_id = self.syntax_capture(&call_capture);
            let call = self.emit_call(&call_capture, owner, &call_syntax_id)?;
            self.edge_if_allowed(
                "Assigns",
                &assignment.id,
                &call.id,
                "assignment_value",
                BTreeMap::new(),
            )?;
        }
        Ok(assignment)
    }

    fn emit_tree_parameters(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        function_node: &TsNode,
        callable: &Node,
    ) -> Result<(), String> {
        for (index, parameter_id) in parameter_child_ids(nodes, function_node)
            .into_iter()
            .enumerate()
        {
            let Some(parameter_node) = nodes.get(&parameter_id) else {
                continue;
            };
            let mut capture = parameter_capture(parameter_node, self.language.as_str());
            if capture.label.is_empty() {
                capture.label = format!("param_{index}");
            }
            let syntax_id = self.syntax_capture(&capture);
            let parameter = self.semantic_node(
                "Parameter",
                &capture,
                &capture.label,
                &callable.id,
                &callable.qualified_name,
                None,
            );
            self.edge_if_allowed(
                "HasParameter",
                &callable.id,
                &parameter.id,
                "callable_parameter",
                BTreeMap::new(),
            )?;
            self.derived_from(&parameter.id, &syntax_id)?;
            if let Some(annotation) =
                parameter_annotation_capture(nodes, parameter_node, self.language.as_str())
            {
                let type_node = self.emit_type_annotation_capture(
                    &annotation,
                    &parameter.id,
                    &parameter.qualified_name,
                )?;
                self.edge_if_allowed(
                    "HasTypeAnnotation",
                    &parameter.id,
                    &type_node.id,
                    "parameter_annotation",
                    BTreeMap::new(),
                )?;
            }
        }
        Ok(())
    }

    fn emit_tree_return_type(
        &mut self,
        nodes: &BTreeMap<usize, TsNode>,
        function_node: &TsNode,
        callable: &Node,
    ) -> Result<(), String> {
        let Some(capture) = return_type_capture(nodes, function_node, self.language.as_str())
        else {
            return Ok(());
        };
        let syntax_id = self.syntax_capture(&capture);
        let return_node = self.semantic_node(
            "ReturnType",
            &capture,
            &capture.label,
            &callable.id,
            &callable.qualified_name,
            None,
        );
        self.edge_if_allowed(
            "HasReturnType",
            &callable.id,
            &return_node.id,
            "callable_return_type",
            BTreeMap::new(),
        )?;
        let type_node = self.emit_type_annotation_capture(
            &capture,
            &return_node.id,
            &return_node.qualified_name,
        )?;
        self.edge_if_allowed(
            "HasTypeAnnotation",
            &return_node.id,
            &type_node.id,
            "return_type_annotation",
            BTreeMap::new(),
        )?;
        self.derived_from(&return_node.id, &syntax_id)?;
        Ok(())
    }

    fn emit_type_annotation_capture(
        &mut self,
        capture: &Capture,
        owner_id: &str,
        owner_qualified_name: &str,
    ) -> Result<Node, String> {
        let syntax_id = self.syntax_capture(capture);
        let type_node = self.semantic_node(
            "TypeAnnotation",
            capture,
            &capture.label,
            owner_id,
            owner_qualified_name,
            None,
        );
        self.emit_reference_edges(&type_node, &type_node.label, "type_annotation")?;
        self.derived_from(&type_node.id, &syntax_id)?;
        Ok(type_node)
    }

    fn emit_type_annotation_label(
        &mut self,
        label: &str,
        source_capture: &Capture,
        owner_id: &str,
        owner_qualified_name: &str,
    ) -> Result<Node, String> {
        let capture = Capture {
            capture_name: "type.annotation".to_string(),
            node_type: "type".to_string(),
            label: label.to_string(),
            text: label.to_string(),
            line_start: source_capture.line_start,
            line_end: source_capture.line_end,
            byte_start: source_capture.byte_start,
            byte_end: source_capture.byte_end,
            fields: Vec::new(),
        };
        let syntax_id = self.syntax_capture(&capture);
        let type_node = self.semantic_node(
            "TypeAnnotation",
            &capture,
            &capture.label,
            owner_id,
            owner_qualified_name,
            None,
        );
        self.emit_reference_edges(&type_node, &type_node.label, "type_annotation")?;
        self.derived_from(&type_node.id, &syntax_id)?;
        Ok(type_node)
    }

    fn emit_parser_like_metadata_fields(
        &mut self,
        node: &TsNode,
        owner: &Owner,
    ) -> Result<(), String> {
        if self.language == "python" {
            return Ok(());
        }
        for field_name in ["_field_types", "_field_descendant_types"] {
            let Some(capture) = parser_like_metadata_capture(node, field_name) else {
                continue;
            };
            let syntax_id = self.syntax_capture(&capture);
            if let Some(table) = table_for_node_type(&capture.node_type, owner) {
                self.emit_simple_semantic(&table, &capture, owner, &syntax_id)?;
            }
        }
        Ok(())
    }

    fn emit_import(
        &mut self,
        capture: &Capture,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let imported = capture.label.clone();
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "imported_name".to_string(),
            JsonValue::String(imported.clone()),
        );
        let semantic = self.semantic_node(
            "ImportDeclaration",
            capture,
            &imported,
            &owner.node_id,
            "",
            Some(metadata),
        );
        self.connect_owner(owner, &semantic)?;
        let import_source_id = if matches!(
            owner.table.as_str(),
            "Repository"
                | "SourceRoot"
                | "File"
                | "Module"
                | "Class"
                | "Function"
                | "Method"
                | "Component"
        ) {
            owner.node_id.clone()
        } else {
            owner.scope_id.clone()
        };
        self.edge_if_allowed(
            "Imports",
            &import_source_id,
            &semantic.id,
            "declares_import",
            BTreeMap::new(),
        )?;
        self.derived_from(&semantic.id, syntax_id)?;
        if !imported.is_empty() {
            let path = self.path.clone();
            let dependency = self.support_node("Dependency", &imported, &imported, &path);
            self.edge(
                "DependsOn",
                &semantic.id,
                &dependency.id,
                "import_dependency",
                BTreeMap::new(),
            )?;
            self.edge(
                "EvidencedBy",
                &dependency.id,
                syntax_id,
                "parser_evidence",
                BTreeMap::new(),
            )?;
        }
        Ok(semantic)
    }

    fn emit_declaration(
        &mut self,
        table: &str,
        capture: &Capture,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let semantic = self.semantic_node(
            table,
            capture,
            &capture.label,
            &owner.node_id,
            &owner.qualified_name,
            None,
        );
        self.connect_owner(owner, &semantic)?;
        self.edge(
            "Defines",
            &owner.node_id,
            &semantic.id,
            &format!("defines_{}", table.to_lowercase()),
            BTreeMap::new(),
        )?;
        if matches!(
            owner.table.as_str(),
            "Module" | "Scope" | "Class" | "Function" | "Method"
        ) {
            self.edge(
                "Declares",
                &owner.node_id,
                &semantic.id,
                &format!("declares_{}", table.to_lowercase()),
                BTreeMap::new(),
            )?;
        }
        self.derived_from(&semantic.id, syntax_id)?;
        Ok(semantic)
    }

    fn emit_call(
        &mut self,
        capture: &Capture,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let call = self.semantic_node(
            "CallExpression",
            capture,
            &capture.label,
            &owner.node_id,
            &owner.qualified_name,
            None,
        );
        self.connect_owner(owner, &call)?;
        if matches!(
            owner.table.as_str(),
            "Function" | "Method" | "APIEndpoint" | "Route" | "Component"
        ) {
            self.edge_if_allowed(
                "Calls",
                &owner.node_id,
                &call.id,
                "body_call",
                BTreeMap::new(),
            )?;
        }
        if let Some(target) = self.emit_reference_edges(&call, &call.label, "call")? {
            self.edge_if_allowed(
                "Calls",
                &call.id,
                &target.id,
                "call_target",
                BTreeMap::new(),
            )?;
        }
        self.derived_from(&call.id, syntax_id)?;
        Ok(call)
    }

    fn emit_reference(
        &mut self,
        capture: &Capture,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let reference = self.semantic_node(
            "Reference",
            capture,
            &capture.label,
            &owner.node_id,
            &owner.qualified_name,
            None,
        );
        self.connect_owner(owner, &reference)?;
        self.emit_reference_edges(&reference, &reference.label, "reference")?;
        self.derived_from(&reference.id, syntax_id)?;
        Ok(reference)
    }

    fn emit_simple_semantic(
        &mut self,
        table: &str,
        capture: &Capture,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<Node, String> {
        let semantic = self.semantic_node(
            table,
            capture,
            &capture.label,
            &owner.node_id,
            &owner.qualified_name,
            None,
        );
        self.connect_owner(owner, &semantic)?;
        if table == "Parameter" {
            self.edge_if_allowed(
                "HasParameter",
                &owner.node_id,
                &semantic.id,
                "captured_parameter",
                BTreeMap::new(),
            )?;
        }
        if table == "ReturnType" {
            self.edge_if_allowed(
                "HasReturnType",
                &owner.node_id,
                &semantic.id,
                "captured_return_type",
                BTreeMap::new(),
            )?;
            let type_node = self.emit_type_annotation_label(
                &semantic.label,
                capture,
                &semantic.id,
                &semantic.qualified_name,
            )?;
            self.edge_if_allowed(
                "HasTypeAnnotation",
                &semantic.id,
                &type_node.id,
                "return_type_annotation",
                BTreeMap::new(),
            )?;
        }
        if table == "TypeAnnotation" {
            self.edge_if_allowed(
                "HasTypeAnnotation",
                &owner.node_id,
                &semantic.id,
                "captured_type_annotation",
                BTreeMap::new(),
            )?;
            self.emit_reference_edges(&semantic, &semantic.label, "type_annotation")?;
        }
        if matches!(table, "DocumentationSource" | "DocumentationChunk") {
            self.edge_if_allowed(
                "Documents",
                &semantic.id,
                &owner.node_id,
                "documents_owner",
                BTreeMap::new(),
            )?;
            self.edge_if_allowed(
                "EvidencedBy",
                &semantic.id,
                syntax_id,
                "parser_evidence",
                BTreeMap::new(),
            )?;
        }
        self.derived_from(&semantic.id, syntax_id)?;
        Ok(semantic)
    }

    fn emit_reference_edges(
        &mut self,
        source: &Node,
        label: &str,
        kind_prefix: &str,
    ) -> Result<Option<Node>, String> {
        let Some(target) = self.resolve_reference_target(label) else {
            return Ok(None);
        };
        if target.id == source.id {
            return Ok(None);
        }
        let mut metadata = BTreeMap::new();
        metadata.insert("label".to_string(), JsonValue::String(label.to_string()));
        metadata.insert(
            "resolver".to_string(),
            JsonValue::String("label".to_string()),
        );
        self.edge_if_allowed(
            "References",
            &source.id,
            &target.id,
            &format!("{}_reference", kind_prefix),
            metadata.clone(),
        )?;
        self.edge_if_allowed(
            "ResolvesTo",
            &source.id,
            &target.id,
            &format!("{}_resolution", kind_prefix),
            metadata,
        )?;
        Ok(Some(target))
    }

    fn resolve_reference_target(&mut self, label: &str) -> Option<Node> {
        let reference_label = label.trim();
        if reference_label.is_empty() {
            return None;
        }
        let short = reference_label
            .rsplit('.')
            .next()
            .unwrap_or(reference_label);
        for candidate in [reference_label, short] {
            let key = symbol_key(candidate);
            if let Some(ids) = self.symbols_by_name.get(&key) {
                for node_id in ids.iter().rev() {
                    if let Some(node) = self.nodes.get(node_id) {
                        return Some(node.clone());
                    }
                }
            }
        }
        Some(self.symbol_node(reference_label))
    }

    fn support_node(&mut self, table: &str, stable_key: &str, label: &str, path: &str) -> Node {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "canonical_key".to_string(),
            JsonValue::String(stable_key.to_string()),
        );
        let node = Node {
            id: graph_id(table, stable_key),
            table: table.to_string(),
            label: label.to_string(),
            kind: table.to_lowercase(),
            language: String::new(),
            path: path.to_string(),
            qualified_name: String::new(),
            scope_id: String::new(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: label.to_string(),
            metadata,
        };
        self.add_node(node)
    }

    fn semantic_node(
        &mut self,
        table: &str,
        capture: &Capture,
        label: &str,
        owner_id: &str,
        owner_qualified_name: &str,
        metadata: Option<BTreeMap<String, JsonValue>>,
    ) -> Node {
        let qualified_name = qualified_name(owner_qualified_name, label);
        let stable_key = [
            self.path.as_str(),
            table,
            qualified_name.as_str(),
            capture.node_type.as_str(),
            &stable_optional_i64(capture.line_start),
            &stable_optional_i64(capture.byte_start),
            label,
        ]
        .join("|");
        let mut node_metadata = BTreeMap::new();
        node_metadata.insert(
            "canonical_key".to_string(),
            JsonValue::String(stable_key.clone()),
        );
        if let Some(extra) = metadata {
            for (key, value) in extra {
                node_metadata.insert(key, value);
            }
        }
        let summary = if matches!(table, "DocumentationSource" | "DocumentationChunk")
            && !capture.text.trim().is_empty()
        {
            capture.text.trim().to_string()
        } else {
            label.to_string()
        };
        let node = Node {
            id: graph_id(table, &stable_key),
            table: table.to_string(),
            label: label.to_string(),
            kind: kind_for(table, &capture.node_type),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name,
            scope_id: owner_id.to_string(),
            line_start: capture.line_start,
            line_end: capture.line_end,
            byte_start: capture.byte_start,
            byte_end: capture.byte_end,
            tree_sitter_node_type: capture.node_type.clone(),
            capture_name: capture.capture_name.clone(),
            summary,
            metadata: node_metadata,
        };
        self.add_node(node)
    }

    fn symbol_node(&mut self, label: &str) -> Node {
        let stable_key = format!("{}|Symbol|{}", self.path, label.trim());
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "canonical_key".to_string(),
            JsonValue::String(stable_key.clone()),
        );
        metadata.insert(
            "resolution".to_string(),
            JsonValue::String("name_placeholder".to_string()),
        );
        let node = Node {
            id: graph_id("Symbol", &stable_key),
            table: "Symbol".to_string(),
            label: label.trim().to_string(),
            kind: "symbol_reference".to_string(),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name: label.trim().to_string(),
            scope_id: String::new(),
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: label.trim().to_string(),
            metadata,
        };
        self.add_node(node)
    }

    fn scope_for(&mut self, owner: &Node) -> Node {
        let stable_key = format!("{}|{}|scope", self.path, owner.id);
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "canonical_key".to_string(),
            JsonValue::String(stable_key.clone()),
        );
        let node = Node {
            id: graph_id("Scope", &stable_key),
            table: "Scope".to_string(),
            label: format!("{} scope", owner.label),
            kind: format!("{}_scope", owner.table.to_lowercase()),
            language: owner.language.clone(),
            path: owner.path.clone(),
            qualified_name: format!(
                "{}.<scope>",
                if owner.qualified_name.is_empty() {
                    &owner.label
                } else {
                    &owner.qualified_name
                }
            ),
            scope_id: owner.id.clone(),
            line_start: owner.line_start,
            line_end: owner.line_end,
            byte_start: owner.byte_start,
            byte_end: owner.byte_end,
            tree_sitter_node_type: String::new(),
            capture_name: String::new(),
            summary: format!("Scope for {}", owner.label),
            metadata,
        };
        self.add_node(node)
    }

    fn syntax_capture(&mut self, capture: &Capture) -> String {
        let stable_key = [
            self.path.as_str(),
            capture.node_type.as_str(),
            &stable_optional_i64(capture.line_start),
            &stable_optional_i64(capture.byte_start),
            capture.label.as_str(),
        ]
        .join("|");
        let syntax_id = graph_id("SyntaxCapture", &stable_key);
        if self.nodes.contains_key(&syntax_id) {
            return syntax_id;
        }
        let mut metadata = BTreeMap::new();
        metadata.insert("canonical_key".to_string(), JsonValue::String(stable_key));
        metadata.insert(
            "fields".to_string(),
            JsonValue::Array(
                capture
                    .fields
                    .iter()
                    .map(|field| JsonValue::String(field.clone()))
                    .collect(),
            ),
        );
        let summary = capture.text.chars().take(160).collect();
        let node = Node {
            id: syntax_id.clone(),
            table: "SyntaxCapture".to_string(),
            label: if capture.capture_name.is_empty() {
                capture.node_type.clone()
            } else {
                capture.capture_name.clone()
            },
            kind: capture.node_type.clone(),
            language: self.language.clone(),
            path: self.path.clone(),
            qualified_name: String::new(),
            scope_id: String::new(),
            line_start: capture.line_start,
            line_end: capture.line_end,
            byte_start: capture.byte_start,
            byte_end: capture.byte_end,
            tree_sitter_node_type: capture.node_type.clone(),
            capture_name: capture.capture_name.clone(),
            summary,
            metadata,
        };
        self.add_node(node);
        syntax_id
    }

    fn connect_owner(&mut self, owner: &Owner, semantic: &Node) -> Result<(), String> {
        self.edge(
            "Contains",
            &owner.node_id,
            &semantic.id,
            &format!("contains_{}", semantic.table.to_lowercase()),
            BTreeMap::new(),
        )?;
        if !owner.scope_id.is_empty() {
            self.edge(
                "Contains",
                &owner.scope_id,
                &semantic.id,
                &format!("scope_contains_{}", semantic.table.to_lowercase()),
                BTreeMap::new(),
            )?;
        }
        Ok(())
    }

    fn derived_from(&mut self, semantic_id: &str, syntax_id: &str) -> Result<(), String> {
        if self.nodes.contains_key(syntax_id) {
            self.edge(
                "DerivedFrom",
                semantic_id,
                syntax_id,
                "parser_capture",
                BTreeMap::new(),
            )?;
        }
        Ok(())
    }

    fn edge_if_allowed(
        &mut self,
        edge_type: &str,
        source_id: &str,
        target_id: &str,
        kind: &str,
        metadata: BTreeMap<String, JsonValue>,
    ) -> Result<(), String> {
        let Some(source) = self.nodes.get(source_id) else {
            return Ok(());
        };
        let Some(target) = self.nodes.get(target_id) else {
            return Ok(());
        };
        if self
            .relation_allowlist
            .allows(edge_type, &source.table, &target.table)
        {
            self.edge(edge_type, source_id, target_id, kind, metadata)?;
        }
        Ok(())
    }

    fn edge(
        &mut self,
        edge_type: &str,
        source_id: &str,
        target_id: &str,
        kind: &str,
        metadata: BTreeMap<String, JsonValue>,
    ) -> Result<Edge, String> {
        let source_table = self.nodes.get(source_id).map(|node| node.table.clone());
        let target_table = self.nodes.get(target_id).map(|node| node.table.clone());
        if let (Some(source_table), Some(target_table)) = (&source_table, &target_table) {
            if !self
                .relation_allowlist
                .allows(edge_type, source_table, target_table)
            {
                return Err(format!(
                    "relation {edge_type} does not allow {source_table} -> {target_table}"
                ));
            }
        }
        let canonical_key = format!("{}|{}|{}|{}", edge_type, source_id, target_id, kind);
        let mut edge_metadata = BTreeMap::new();
        edge_metadata.insert(
            "canonical_key".to_string(),
            JsonValue::String(canonical_key.clone()),
        );
        for (key, value) in metadata {
            edge_metadata.insert(key, value);
        }
        let edge = Edge {
            id: graph_id("edge", &canonical_key),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: kind.to_string(),
            metadata: edge_metadata,
        };
        self.edges
            .entry(edge.id.clone())
            .or_insert_with(|| edge.clone());
        Ok(edge)
    }

    fn add_node(&mut self, node: Node) -> Node {
        self.nodes
            .entry(node.id.clone())
            .or_insert_with(|| node.clone());
        let added = self.nodes.get(&node.id).cloned().unwrap_or(node);
        self.register_resolvable(&added);
        added
    }

    fn register_resolvable(&mut self, node: &Node) {
        if !matches!(
            node.table.as_str(),
            "Symbol"
                | "Module"
                | "Class"
                | "Function"
                | "Method"
                | "Variable"
                | "Constant"
                | "Dependency"
        ) {
            return;
        }
        for key in [
            node.label.as_str(),
            node.qualified_name.as_str(),
            imported_name(node).as_str(),
        ] {
            let normalized = symbol_key(key);
            if normalized.is_empty() {
                continue;
            }
            let ids = self.symbols_by_name.entry(normalized).or_default();
            if !ids.contains(&node.id) {
                ids.push(node.id.clone());
            }
        }
    }

    fn encode_output(&self) -> String {
        let mut lines = vec![
            encode_record(&["META", &hex("source_path"), &hex_json_string(&self.path)]),
            encode_record(&["META", &hex("language"), &hex_json_string(&self.language)]),
            encode_record(&[
                "META",
                &hex("source_root"),
                &hex_json_string(&self.source_root),
            ]),
        ];
        let mut nodes: Vec<&Node> = self.nodes.values().collect();
        nodes.sort_by(|left, right| {
            (left.table.as_str(), left.id.as_str()).cmp(&(right.table.as_str(), right.id.as_str()))
        });
        for node in nodes {
            lines.push(node.encode());
        }
        let mut edges: Vec<&Edge> = self.edges.values().collect();
        edges.sort_by(|left, right| {
            (left.edge_type.as_str(), left.id.as_str())
                .cmp(&(right.edge_type.as_str(), right.id.as_str()))
        });
        for edge in edges {
            lines.push(edge.encode());
        }
        lines.join("\n") + "\n"
    }

    fn typed_rows(&self) -> BuiltGraphRows {
        let mut nodes: Vec<&Node> = self.nodes.values().collect();
        nodes.sort_by(|left, right| {
            (left.table.as_str(), left.id.as_str()).cmp(&(right.table.as_str(), right.id.as_str()))
        });
        let mut edges: Vec<&Edge> = self.edges.values().collect();
        edges.sort_by(|left, right| {
            (left.edge_type.as_str(), left.id.as_str())
                .cmp(&(right.edge_type.as_str(), right.id.as_str()))
        });
        BuiltGraphRows {
            nodes: nodes.into_iter().map(GraphNodeRow::from).collect(),
            edges: edges.into_iter().map(GraphEdgeRow::from).collect(),
        }
    }
}

impl From<&Node> for GraphNodeRow {
    fn from(node: &Node) -> Self {
        Self {
            id: node.id.clone(),
            table: node.table.clone(),
            label: node.label.clone(),
            kind: node.kind.clone(),
            language: node.language.clone(),
            path: node.path.clone(),
            qualified_name: node.qualified_name.clone(),
            scope_id: node.scope_id.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            byte_start: node.byte_start,
            byte_end: node.byte_end,
            tree_sitter_node_type: node.tree_sitter_node_type.clone(),
            capture_name: node.capture_name.clone(),
            summary: node.summary.clone(),
            metadata: json_object(&node.metadata),
        }
    }
}

impl From<&Edge> for GraphEdgeRow {
    fn from(edge: &Edge) -> Self {
        Self {
            id: edge.id.clone(),
            edge_type: edge.edge_type.clone(),
            source_id: edge.source_id.clone(),
            target_id: edge.target_id.clone(),
            kind: edge.kind.clone(),
            metadata: json_object(&edge.metadata),
        }
    }
}

impl Node {
    fn encode(&self) -> String {
        encode_record(&[
            "NODE",
            &hex(&self.id),
            &hex(&self.table),
            &hex(&self.label),
            &hex(&self.kind),
            &hex(&self.language),
            &hex(&self.path),
            &hex(&self.qualified_name),
            &hex(&self.scope_id),
            &optional_i64(self.line_start),
            &optional_i64(self.line_end),
            &optional_i64(self.byte_start),
            &optional_i64(self.byte_end),
            &hex(&self.tree_sitter_node_type),
            &hex(&self.capture_name),
            &hex(&self.summary),
            &hex(&json_object(&self.metadata)),
        ])
    }
}

impl Edge {
    fn encode(&self) -> String {
        encode_record(&[
            "EDGE",
            &hex(&self.id),
            &hex(&self.edge_type),
            &hex(&self.source_id),
            &hex(&self.target_id),
            &hex(&self.kind),
            "1.0",
            "",
            "",
            "",
            "",
            &hex(&json_object(&self.metadata)),
        ])
    }
}

type BulkRow = BTreeMap<String, String>;
type BulkRowsById = BTreeMap<String, BulkRow>;
type ConnectorKey = (String, String, String);
type ConnectorRowKey = (String, String, String);

struct BulkStaging {
    staging_dir: PathBuf,
    node_tables: Vec<String>,
    edge_tables: Vec<String>,
    nodes: BTreeMap<String, BulkRowsById>,
    edges: BTreeMap<String, BulkRowsById>,
    connectors: BTreeMap<ConnectorKey, BTreeMap<ConnectorRowKey, ConnectorRow>>,
}

struct ConnectorRow {
    from_id: String,
    to_id: String,
    role: String,
}

struct BulkStagingOutput {
    copy_statements: Vec<String>,
    node_rows: usize,
    edge_rows: usize,
    connector_rows: usize,
}

fn run_bulk_staging(input: &str) -> Result<(), String> {
    let output = parse_bulk_staging(input)?.write()?;
    println!(
        "RESULT\t{}\t{}\t{}",
        output.node_rows, output.edge_rows, output.connector_rows
    );
    for statement in output.copy_statements {
        println!("COPY\t{}", hex(&statement));
    }
    Ok(())
}

fn parse_bulk_staging(input: &str) -> Result<BulkStaging, String> {
    let mut staging = BulkStaging {
        staging_dir: PathBuf::new(),
        node_tables: Vec::new(),
        edge_tables: Vec::new(),
        nodes: BTreeMap::new(),
        edges: BTreeMap::new(),
        connectors: BTreeMap::new(),
    };

    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("BULK") if parts.len() == 2 => {
                staging.staging_dir = PathBuf::from(unhex(parts[1])?);
            }
            Some("TABLE") if parts.len() == 3 => {
                let kind = unhex(parts[1])?;
                let table = unhex(parts[2])?;
                match kind.as_str() {
                    "node" => staging.node_tables.push(table),
                    "edge" => staging.edge_tables.push(table),
                    _ => return Err(format!("unknown bulk table kind: {kind}")),
                }
            }
            Some("NROW") if parts.len() >= 4 => {
                let table = unhex(parts[1])?;
                let row_id = unhex(parts[2])?;
                let fields = decode_bulk_fields(&parts[3..])?;
                merge_bulk_row(staging.nodes.entry(table).or_default(), row_id, fields);
            }
            Some("EROW") if parts.len() >= 8 => {
                let table = unhex(parts[1])?;
                let row_id = unhex(parts[2])?;
                let source_id = unhex(parts[3])?;
                let target_id = unhex(parts[4])?;
                let source_table = unhex(parts[5])?;
                let target_table = unhex(parts[6])?;
                let fields = decode_bulk_fields(&parts[7..])?;
                merge_bulk_row(
                    staging.edges.entry(table.clone()).or_default(),
                    row_id.clone(),
                    fields,
                );
                staging.add_connector(
                    format!("FROM_{table}"),
                    source_table,
                    table.clone(),
                    source_id,
                    row_id.clone(),
                    "source".to_string(),
                );
                staging.add_connector(
                    format!("TO_{table}"),
                    table,
                    target_table,
                    row_id,
                    target_id,
                    "target".to_string(),
                );
            }
            Some(kind) => return Err(format!("invalid bulk input record: {kind}")),
            None => {}
        }
    }

    if staging.staging_dir.as_os_str().is_empty() {
        return Err("bulk staging directory is missing".to_string());
    }
    Ok(staging)
}

fn decode_bulk_fields(parts: &[&str]) -> Result<BulkRow, String> {
    let Some(count_text) = parts.first() else {
        return Err("bulk row field count is missing".to_string());
    };
    let count = count_text
        .parse::<usize>()
        .map_err(|error| format!("invalid bulk row field count: {error}"))?;
    if parts.len() != 1 + (count * 2) {
        return Err(format!(
            "bulk row field count mismatch: expected {}, got {}",
            count * 2,
            parts.len().saturating_sub(1)
        ));
    }
    let mut fields = BTreeMap::new();
    for index in 0..count {
        let key = unhex(parts[1 + (index * 2)])?;
        let token = unhex(parts[2 + (index * 2)])?;
        fields.insert(key, token);
    }
    Ok(fields)
}

impl BulkStaging {
    fn add_connector(
        &mut self,
        table: String,
        from_type: String,
        to_type: String,
        from_id: String,
        to_id: String,
        role: String,
    ) {
        let rows = self
            .connectors
            .entry((table, from_type, to_type))
            .or_default();
        rows.entry((from_id.clone(), to_id.clone(), role.clone()))
            .or_insert(ConnectorRow {
                from_id,
                to_id,
                role,
            });
    }

    fn write(&self) -> Result<BulkStagingOutput, String> {
        fs::create_dir_all(&self.staging_dir).map_err(|error| error.to_string())?;

        let mut copy_statements = Vec::new();
        let mut node_rows = 0;
        let mut edge_rows = 0;
        let mut connector_rows = 0;

        for table in &self.node_tables {
            let Some(rows) = self.nodes.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, rows.values())?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            node_rows += rows.len();
        }

        for table in &self.edge_tables {
            let Some(rows) = self.edges.get(table) else {
                continue;
            };
            if rows.is_empty() {
                continue;
            }
            let path = self
                .staging_dir
                .join(format!("{}.json", stage_file_stem(table)));
            write_json_rows(&path, rows.values())?;
            copy_statements.push(format!("COPY `{}` FROM \"{}\";", table, copy_path(&path)));
            edge_rows += rows.len();
        }

        for relation in &self.edge_tables {
            for connector_table in [format!("FROM_{relation}"), format!("TO_{relation}")] {
                for ((table, from_type, to_type), rows) in &self.connectors {
                    if table != &connector_table || rows.is_empty() {
                        continue;
                    }
                    let path = self.staging_dir.join(format!(
                        "{}__{}__{}.csv",
                        stage_file_stem(table),
                        stage_file_stem(from_type),
                        stage_file_stem(to_type)
                    ));
                    write_csv_rows(&path, rows.values())?;
                    copy_statements.push(format!(
                        "COPY `{}` FROM \"{}\" (header=true, from=\"{}\", to=\"{}\");",
                        table,
                        copy_path(&path),
                        from_type,
                        to_type
                    ));
                    connector_rows += rows.len();
                }
            }
        }

        Ok(BulkStagingOutput {
            copy_statements,
            node_rows,
            edge_rows,
            connector_rows,
        })
    }
}

fn merge_bulk_row(rows: &mut BulkRowsById, row_id: String, incoming: BulkRow) {
    let Some(existing) = rows.get_mut(&row_id) else {
        rows.insert(row_id, incoming);
        return;
    };

    for (key, value) in incoming {
        if !json_token_is_empty(&value) {
            let should_replace = match existing.get(&key) {
                Some(current) => json_token_is_empty(current),
                None => true,
            };
            if should_replace {
                existing.insert(key, value);
            }
        }
    }
}

fn json_token_is_empty(value: &str) -> bool {
    matches!(value, "null" | "\"\"" | "{}" | "[]")
}

fn write_json_rows<'a>(
    path: &Path,
    rows: impl Iterator<Item = &'a BTreeMap<String, String>>,
) -> Result<(), String> {
    let mut output = String::from("[");
    for (row_index, row) in rows.enumerate() {
        if row_index > 0 {
            output.push(',');
        }
        output.push('{');
        for (field_index, (key, value)) in row.iter().enumerate() {
            if field_index > 0 {
                output.push(',');
            }
            output.push_str(&json_string(key));
            output.push(':');
            output.push_str(value);
        }
        output.push('}');
    }
    output.push_str("]\n");
    fs::write(path, output).map_err(|error| error.to_string())
}

fn write_csv_rows<'a>(
    path: &Path,
    rows: impl Iterator<Item = &'a ConnectorRow>,
) -> Result<(), String> {
    let mut output = String::from("from_id,to_id,role\r\n");
    for row in rows {
        output.push_str(&csv_field(&row.from_id));
        output.push(',');
        output.push_str(&csv_field(&row.to_id));
        output.push(',');
        output.push_str(&csv_field(&row.role));
        output.push_str("\r\n");
    }
    fs::write(path, output).map_err(|error| error.to_string())
}

fn csv_field(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn stage_file_stem(name: &str) -> String {
    let stem = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if stem.is_empty() {
        "table".to_string()
    } else {
        stem
    }
}

fn copy_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

struct TsNormInput {
    language: String,
    root_types: Vec<String>,
    mappings: Vec<TsMapping>,
    nodes: BTreeMap<usize, TsNode>,
    root_id: usize,
}

#[allow(dead_code)]
struct TreeGraphInput {
    meta: BTreeMap<String, String>,
    nodes: BTreeMap<usize, TsNode>,
    root_id: usize,
}

struct TsMapping {
    capture_name: String,
    parser_node_types: Vec<String>,
    context_rule: String,
}

struct TsNode {
    id: usize,
    parent_id: Option<usize>,
    node_type: String,
    text: String,
    line_start: Option<i64>,
    line_end: Option<i64>,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    capture_name: String,
    fields: BulkRow,
    field_types: BTreeMap<String, String>,
    field_descendant_types: BTreeMap<String, Vec<String>>,
    children: Vec<usize>,
}

struct TsNormOutput {
    diagnostics: Vec<String>,
    captures: Vec<(String, usize)>,
    nodes: BTreeMap<usize, TsNode>,
    root_id: usize,
}

fn run_tree_sitter_normalization(input: &str) -> Result<(), String> {
    let parsed = parse_tree_sitter_normalization(input)?;
    let mut output = normalize_tree_sitter(parsed)?;
    println!("RESULT\t{}", output.root_id);
    for diagnostic in output.diagnostics {
        println!("DIAG\t{}", hex(&diagnostic));
    }
    for node in output.nodes.values_mut() {
        println!("{}", encode_ts_node(node));
    }
    for (capture_name, node_id) in output.captures {
        println!("CAP\t{}\t{}", hex(&capture_name), node_id);
    }
    Ok(())
}

fn parse_tree_sitter_normalization(input: &str) -> Result<TsNormInput, String> {
    let mut language = String::new();
    let mut root_types = Vec::new();
    let mut mappings = Vec::new();
    let mut nodes = BTreeMap::new();
    let mut root_id = None;

    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("TSNORM") if parts.len() == 1 => {}
            Some("LANGUAGE") if parts.len() == 2 => {
                language = unhex(parts[1])?;
            }
            Some("ROOT_TYPES") => {
                root_types = decode_hex_list(&parts[1..])?;
            }
            Some("MAPPING") if parts.len() >= 4 => {
                let capture_name = unhex(parts[1])?;
                let context_rule = unhex(parts[2])?;
                let parser_node_types = decode_counted_hex_list(&parts[3..])?;
                mappings.push(TsMapping {
                    capture_name,
                    parser_node_types,
                    context_rule,
                });
            }
            Some("NODE") if parts.len() >= 12 => {
                let node = decode_ts_node(&parts[1..])?;
                if node.parent_id.is_none() {
                    root_id = Some(node.id);
                }
                nodes.insert(node.id, node);
            }
            Some(kind) => return Err(format!("invalid tree-sitter normalization record: {kind}")),
            None => {}
        }
    }

    for node_id in nodes.keys().copied().collect::<Vec<_>>() {
        let Some(parent_id) = nodes.get(&node_id).and_then(|node| node.parent_id) else {
            continue;
        };
        let Some(parent) = nodes.get_mut(&parent_id) else {
            return Err(format!(
                "tree-sitter node {node_id} references missing parent {parent_id}"
            ));
        };
        parent.children.push(node_id);
    }

    Ok(TsNormInput {
        language,
        root_types,
        mappings,
        nodes,
        root_id: root_id.unwrap_or(0),
    })
}

fn decode_hex_list(parts: &[&str]) -> Result<Vec<String>, String> {
    let Some(count_text) = parts.first() else {
        return Err("hex list count is missing".to_string());
    };
    let count = count_text
        .parse::<usize>()
        .map_err(|error| format!("invalid hex list count: {error}"))?;
    if parts.len() != count + 1 {
        return Err(format!(
            "hex list count mismatch: expected {count}, got {}",
            parts.len().saturating_sub(1)
        ));
    }
    parts[1..].iter().map(|value| unhex(value)).collect()
}

fn decode_counted_hex_list(parts: &[&str]) -> Result<Vec<String>, String> {
    decode_hex_list(parts)
}

fn decode_ts_node(parts: &[&str]) -> Result<TsNode, String> {
    let id = parts[0]
        .parse::<usize>()
        .map_err(|error| format!("invalid tree-sitter node id: {error}"))?;
    let parent_id = if parts[1].is_empty() {
        None
    } else {
        Some(
            parts[1]
                .parse::<usize>()
                .map_err(|error| format!("invalid tree-sitter parent id: {error}"))?,
        )
    };
    let node_type = unhex(parts[2])?;
    let text = unhex(parts[3])?;
    let line_start = parse_optional_i64(parts[4])?;
    let line_end = parse_optional_i64(parts[5])?;
    let byte_start = parse_optional_i64(parts[6])?;
    let byte_end = parse_optional_i64(parts[7])?;
    let capture_name = unhex(parts[8])?;
    let mut cursor = 9;
    let fields = decode_ts_field_tokens(parts, &mut cursor)?;
    let field_types = decode_ts_field_types(parts, &mut cursor)?;
    let field_descendant_types = decode_ts_field_descendant_types(parts, &mut cursor)?;
    if cursor != parts.len() {
        return Err("tree-sitter node has trailing fields".to_string());
    }
    Ok(TsNode {
        id,
        parent_id,
        node_type,
        text,
        line_start,
        line_end,
        byte_start,
        byte_end,
        capture_name,
        fields,
        field_types,
        field_descendant_types,
        children: Vec::new(),
    })
}

fn decode_ts_field_tokens(parts: &[&str], cursor: &mut usize) -> Result<BulkRow, String> {
    let count = parse_count(parts, cursor, "tree-sitter field")?;
    let mut fields = BTreeMap::new();
    for _ in 0..count {
        let key = next_unhex(parts, cursor, "field key")?;
        let token = next_unhex(parts, cursor, "field token")?;
        fields.insert(key, token);
    }
    Ok(fields)
}

fn decode_ts_field_types(
    parts: &[&str],
    cursor: &mut usize,
) -> Result<BTreeMap<String, String>, String> {
    let count = parse_count(parts, cursor, "tree-sitter field type")?;
    let mut fields = BTreeMap::new();
    for _ in 0..count {
        let key = next_unhex(parts, cursor, "field type key")?;
        let value = next_unhex(parts, cursor, "field type value")?;
        fields.insert(key, value);
    }
    Ok(fields)
}

fn decode_ts_field_descendant_types(
    parts: &[&str],
    cursor: &mut usize,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let count = parse_count(parts, cursor, "tree-sitter field descendant")?;
    let mut fields = BTreeMap::new();
    for _ in 0..count {
        let key = next_unhex(parts, cursor, "field descendant key")?;
        let values = decode_hex_list_from_cursor(parts, cursor)?;
        fields.insert(key, values);
    }
    Ok(fields)
}

fn parse_count(parts: &[&str], cursor: &mut usize, label: &str) -> Result<usize, String> {
    let Some(value) = parts.get(*cursor) else {
        return Err(format!("{label} count is missing"));
    };
    *cursor += 1;
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {label} count: {error}"))
}

fn next_unhex(parts: &[&str], cursor: &mut usize, label: &str) -> Result<String, String> {
    let Some(value) = parts.get(*cursor) else {
        return Err(format!("{label} is missing"));
    };
    *cursor += 1;
    unhex(value)
}

fn decode_hex_list_from_cursor(parts: &[&str], cursor: &mut usize) -> Result<Vec<String>, String> {
    let count = parse_count(parts, cursor, "nested hex list")?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(next_unhex(parts, cursor, "nested hex value")?);
    }
    Ok(values)
}

fn normalize_tree_sitter(input: TsNormInput) -> Result<TsNormOutput, String> {
    if !input.nodes.contains_key(&input.root_id) {
        return Err("tree-sitter root node is missing".to_string());
    }
    let mut diagnostics = Vec::new();
    if !input.root_types.is_empty() {
        let root_type = input
            .nodes
            .get(&input.root_id)
            .map(|node| node.node_type.as_str())
            .unwrap_or("");
        if !input.root_types.iter().any(|item| item == root_type) {
            if input.language.is_empty() {
                diagnostics.push(format!("Unexpected root node {root_type}"));
            } else {
                diagnostics.push(format!(
                    "Unexpected root node {root_type} for {}",
                    input.language
                ));
            }
        }
    }

    let mut nodes = input.nodes;
    let mut captures = Vec::new();
    mark_ts_captures(
        input.root_id,
        &mut nodes,
        &input.mappings,
        Vec::new(),
        &mut captures,
    )?;
    Ok(TsNormOutput {
        diagnostics,
        captures,
        nodes,
        root_id: input.root_id,
    })
}

fn mark_ts_captures(
    node_id: usize,
    nodes: &mut BTreeMap<usize, TsNode>,
    mappings: &[TsMapping],
    ancestors: Vec<String>,
    captures: &mut Vec<(String, usize)>,
) -> Result<(), String> {
    let (node_type, children) = {
        let Some(node) = nodes.get(&node_id) else {
            return Err(format!("tree-sitter node {node_id} is missing"));
        };
        (node.node_type.clone(), node.children.clone())
    };
    let mut child_ancestors = ancestors.clone();
    child_ancestors.push(node_type);
    for child_id in children {
        mark_ts_captures(child_id, nodes, mappings, child_ancestors.clone(), captures)?;
    }
    let capture_name = {
        let node = nodes
            .get(&node_id)
            .ok_or_else(|| format!("tree-sitter node {node_id} is missing"))?;
        mapping_for_ts_node(node, mappings, &ancestors).map(|mapping| mapping.capture_name.clone())
    };
    if let Some(capture_name) = capture_name {
        if let Some(node) = nodes.get_mut(&node_id) {
            node.capture_name = capture_name.clone();
        }
        captures.push((capture_name, node_id));
    }
    Ok(())
}

fn mapping_for_ts_node<'a>(
    node: &TsNode,
    mappings: &'a [TsMapping],
    ancestors: &[String],
) -> Option<&'a TsMapping> {
    let candidates: Vec<&TsMapping> = mappings
        .iter()
        .filter(|mapping| {
            mapping
                .parser_node_types
                .iter()
                .any(|node_type| node_type == &node.node_type)
        })
        .collect();
    for mapping in &candidates {
        if !mapping.context_rule.is_empty()
            && ts_context_rule_matches(&mapping.context_rule, node, ancestors)
        {
            return Some(mapping);
        }
    }
    candidates
        .into_iter()
        .find(|mapping| mapping.context_rule.is_empty())
}

fn ts_context_rule_matches(rule: &str, node: &TsNode, ancestors: &[String]) -> bool {
    let normalized = rule.trim().to_lowercase();
    if let Some(expected) = normalized.strip_prefix("inside ") {
        return ancestors
            .iter()
            .any(|ancestor| ts_context_name_matches(ancestor, expected));
    }
    if let Some(expected_type) = normalized.strip_prefix("type is ") {
        return ts_field_type_matches(node, "type", expected_type);
    }
    if normalized == "qualified declarator" {
        return ts_field_descendant_has(node, "declarator", "qualified_identifier");
    }
    if normalized == "function declarator" {
        return ts_field_type_matches(node, "declarator", "function_declarator")
            || ts_field_descendant_has(node, "declarator", "function_declarator");
    }
    false
}

fn ts_context_name_matches(node_type: &str, expected: &str) -> bool {
    match expected {
        "impl" => node_type == "impl_item",
        "class" => matches!(node_type, "class_specifier" | "struct_specifier"),
        value => node_type == value,
    }
}

fn ts_field_type_matches(node: &TsNode, field_name: &str, expected_type: &str) -> bool {
    node.field_types
        .get(field_name)
        .is_some_and(|value| value == expected_type)
}

fn ts_field_descendant_has(node: &TsNode, field_name: &str, expected_type: &str) -> bool {
    node.field_descendant_types
        .get(field_name)
        .is_some_and(|values| values.iter().any(|value| value == expected_type))
}

fn encode_ts_node(node: &TsNode) -> String {
    let mut parts = vec![
        "NODE".to_string(),
        node.id.to_string(),
        node.parent_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
        hex(&node.node_type),
        hex(&node.text),
        optional_i64(node.line_start),
        optional_i64(node.line_end),
        optional_i64(node.byte_start),
        optional_i64(node.byte_end),
        hex(&node.capture_name),
        node.fields.len().to_string(),
    ];
    for (key, value) in &node.fields {
        parts.push(hex(key));
        parts.push(hex(value));
    }
    parts.join("\t")
}

struct ScanInput {
    source_root: PathBuf,
    expected_schema_version: i64,
    expected_ontology: String,
    expected_parser_version: String,
    manifest_schema_version: Option<i64>,
    manifest_ontology: String,
    manifest_parser_version: String,
    suffix_to_language: BTreeMap<String, String>,
    excluded_parts: BTreeSet<String>,
    previous_files: BTreeMap<String, ScanManifestEntry>,
}

#[derive(Clone)]
struct ScanManifestEntry {
    path: String,
    content_hash: String,
    language: String,
}

struct ScanSnapshot {
    path: String,
    absolute_path: String,
    content_hash: String,
    language: String,
}

struct ScanDiff {
    added: Vec<String>,
    modified: Vec<String>,
    unchanged: Vec<String>,
    deleted: Vec<String>,
    force_rebuild: bool,
}

struct ScanOutput {
    snapshots: BTreeMap<String, ScanSnapshot>,
    diagnostics: Vec<String>,
    diff: Option<ScanDiff>,
}

fn run_scan_diff(input: &str) -> Result<(), String> {
    let output = execute_scan_diff(parse_scan_diff(input)?)?;
    println!("RESULT\t{}", output.snapshots.len());
    for snapshot in output.snapshots.values() {
        println!("{}", encode_scan_snapshot(snapshot));
    }
    for diagnostic in output.diagnostics {
        println!("DIAG\t{}", hex(&diagnostic));
    }
    if let Some(diff) = output.diff {
        println!("{}", encode_scan_diff(&diff));
    }
    Ok(())
}

fn parse_scan_diff(input: &str) -> Result<ScanInput, String> {
    let mut source_root = PathBuf::new();
    let mut expected_schema_version = 0;
    let mut expected_ontology = String::new();
    let mut expected_parser_version = String::new();
    let mut manifest_schema_version = None;
    let mut manifest_ontology = String::new();
    let mut manifest_parser_version = String::new();
    let mut suffix_to_language = BTreeMap::new();
    let mut excluded_parts = BTreeSet::new();
    let mut previous_files = BTreeMap::new();

    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("SCAN") if parts.len() == 1 => {}
            Some("ROOT") if parts.len() == 2 => {
                source_root = PathBuf::from(unhex(parts[1])?);
            }
            Some("EXPECTED") if parts.len() == 4 => {
                expected_schema_version = parts[1].parse::<i64>().map_err(|error| {
                    format!("invalid expected manifest schema version: {error}")
                })?;
                expected_ontology = unhex(parts[2])?;
                expected_parser_version = unhex(parts[3])?;
            }
            Some("MANIFEST") if parts.len() == 4 => {
                manifest_schema_version = Some(
                    parts[1]
                        .parse::<i64>()
                        .map_err(|error| format!("invalid manifest schema version: {error}"))?,
                );
                manifest_ontology = unhex(parts[2])?;
                manifest_parser_version = unhex(parts[3])?;
            }
            Some("SUFFIX") if parts.len() == 3 => {
                suffix_to_language.insert(unhex(parts[1])?, unhex(parts[2])?);
            }
            Some("EXCLUDE") if parts.len() == 2 => {
                excluded_parts.insert(unhex(parts[1])?);
            }
            Some("MENTRY") if parts.len() == 4 => {
                let entry = ScanManifestEntry {
                    path: unhex(parts[1])?,
                    content_hash: unhex(parts[2])?,
                    language: unhex(parts[3])?,
                };
                previous_files.insert(entry.path.clone(), entry);
            }
            Some(kind) => return Err(format!("invalid scan diff record: {kind}")),
            None => {}
        }
    }

    if source_root.as_os_str().is_empty() {
        return Err("scan source root is missing".to_string());
    }
    Ok(ScanInput {
        source_root,
        expected_schema_version,
        expected_ontology,
        expected_parser_version,
        manifest_schema_version,
        manifest_ontology,
        manifest_parser_version,
        suffix_to_language,
        excluded_parts,
        previous_files,
    })
}

fn execute_scan_diff(input: ScanInput) -> Result<ScanOutput, String> {
    let mut snapshots = BTreeMap::new();
    let mut diagnostics = Vec::new();
    scan_source_root(&input, &input.source_root, &mut snapshots, &mut diagnostics)?;
    let diff = if input.manifest_schema_version.is_some() {
        Some(diff_scan_manifest(&input, &snapshots))
    } else {
        None
    };
    Ok(ScanOutput {
        snapshots,
        diagnostics,
        diff,
    })
}

fn scan_source_root(
    input: &ScanInput,
    current_root: &Path,
    snapshots: &mut BTreeMap<String, ScanSnapshot>,
    diagnostics: &mut Vec<String>,
) -> Result<(), String> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(current_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            if !scan_part_is_excluded(&input.excluded_parts, &name) {
                directories.push(path);
            }
        } else if file_type.is_file()
            && !scan_path_is_excluded(&input.source_root, &path, &input.excluded_parts)?
        {
            files.push(path);
        }
    }
    directories.sort();
    files.sort();
    for path in files {
        let relative_path = scan_relative_path(&input.source_root, &path)?;
        let language = scan_language_for_path(&path, &input.suffix_to_language);
        if language.is_empty() {
            snapshots.insert(
                relative_path.clone(),
                ScanSnapshot {
                    path: relative_path.clone(),
                    absolute_path: path.to_string_lossy().to_string(),
                    content_hash: String::new(),
                    language,
                },
            );
            diagnostics.push(format!("Skipped unsupported file: {relative_path}"));
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| error.to_string())?;
        snapshots.insert(
            relative_path.clone(),
            ScanSnapshot {
                path: relative_path,
                absolute_path: path.to_string_lossy().to_string(),
                content_hash: sha256_hex(&bytes),
                language,
            },
        );
    }
    for directory in directories {
        scan_source_root(input, &directory, snapshots, diagnostics)?;
    }
    Ok(())
}

fn diff_scan_manifest(input: &ScanInput, snapshots: &BTreeMap<String, ScanSnapshot>) -> ScanDiff {
    let supported: BTreeMap<String, &ScanSnapshot> = snapshots
        .iter()
        .filter(|(_, snapshot)| !snapshot.language.is_empty())
        .map(|(path, snapshot)| (path.clone(), snapshot))
        .collect();
    if !input.manifest_is_compatible() {
        return ScanDiff {
            added: supported.keys().cloned().collect(),
            modified: Vec::new(),
            unchanged: Vec::new(),
            deleted: input
                .previous_files
                .keys()
                .filter(|path| !supported.contains_key(*path))
                .cloned()
                .collect(),
            force_rebuild: true,
        };
    }

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut unchanged = Vec::new();
    for (path, snapshot) in &supported {
        match input.previous_files.get(path) {
            None => added.push(path.clone()),
            Some(previous)
                if previous.content_hash != snapshot.content_hash
                    || previous.language != snapshot.language =>
            {
                modified.push(path.clone());
            }
            Some(_) => unchanged.push(path.clone()),
        }
    }
    let deleted = input
        .previous_files
        .keys()
        .filter(|path| !supported.contains_key(*path))
        .cloned()
        .collect();
    ScanDiff {
        added,
        modified,
        unchanged,
        deleted,
        force_rebuild: false,
    }
}

impl ScanInput {
    fn manifest_is_compatible(&self) -> bool {
        self.manifest_schema_version == Some(self.expected_schema_version)
            && self.manifest_ontology == self.expected_ontology
            && self.manifest_parser_version == self.expected_parser_version
    }
}

fn scan_relative_path(source_root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(source_root)
        .map_err(|error| error.to_string())?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/"))
}

fn scan_path_is_excluded(
    source_root: &Path,
    path: &Path,
    excluded_parts: &BTreeSet<String>,
) -> Result<bool, String> {
    let relative = path
        .strip_prefix(source_root)
        .map_err(|error| error.to_string())?;
    Ok(relative.components().any(|component| {
        scan_part_is_excluded(
            excluded_parts,
            component.as_os_str().to_string_lossy().as_ref(),
        )
    }))
}

fn scan_part_is_excluded(excluded_parts: &BTreeSet<String>, part: &str) -> bool {
    excluded_parts.contains(part) || part.ends_with(".egg-info")
}

fn scan_language_for_path(path: &Path, suffix_to_language: &BTreeMap<String, String>) -> String {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return String::new();
    };
    let Some(index) = name.rfind('.') else {
        return String::new();
    };
    if index == 0 {
        return String::new();
    }
    suffix_to_language
        .get(&name[index..])
        .cloned()
        .unwrap_or_default()
}

fn encode_scan_snapshot(snapshot: &ScanSnapshot) -> String {
    encode_record(&[
        "SNAP",
        &hex(&snapshot.path),
        &hex(&snapshot.absolute_path),
        &hex(&snapshot.content_hash),
        &hex(&snapshot.language),
    ])
}

fn encode_scan_diff(diff: &ScanDiff) -> String {
    let mut parts = vec![
        "DIFF".to_string(),
        if diff.force_rebuild { "1" } else { "0" }.to_string(),
    ];
    extend_counted_hex(&mut parts, &diff.added);
    extend_counted_hex(&mut parts, &diff.modified);
    extend_counted_hex(&mut parts, &diff.unchanged);
    extend_counted_hex(&mut parts, &diff.deleted);
    parts.join("\t")
}

fn extend_counted_hex(parts: &mut Vec<String>, values: &[String]) {
    parts.push(values.len().to_string());
    for value in values {
        parts.push(hex(value));
    }
}

const DECLARATION_TABLES: &[&str] = &[
    "Symbol",
    "Module",
    "Class",
    "Function",
    "Method",
    "Variable",
    "Constant",
    "ClassAttribute",
    "InstanceAttribute",
    "Property",
    "Parameter",
    "Dependency",
    "APIEndpoint",
    "Component",
    "TypeAlias",
];

const REFERENCE_TABLES: &[&str] = &[
    "Reference",
    "ImportDeclaration",
    "CallExpression",
    "TypeAnnotation",
    "Decorator",
];

struct SemanticInput {
    relation_specs: BTreeMap<String, RelationSpec>,
    nodes: BTreeMap<String, SemNode>,
    node_order: Vec<String>,
    edges: BTreeMap<String, SemEdge>,
    edge_order: Vec<String>,
}

struct RelationSpec {
    source_types: BTreeSet<String>,
    target_types: BTreeSet<String>,
}

#[derive(Clone)]
struct SemNode {
    graph_index: usize,
    id: String,
    table: String,
    label: String,
    language: String,
    path: String,
    qualified_name: String,
    scope_id: String,
    imported_name: String,
    owner_node_id: String,
    typed_node_id: String,
}

#[derive(Clone)]
struct SemEdge {
    graph_index: usize,
    id: String,
    edge_type: String,
    source_id: String,
    target_id: String,
    kind: String,
    confidence: f64,
    metadata: BTreeMap<String, String>,
}

#[derive(Clone)]
struct SemSymbol {
    symbol_id: String,
    name: String,
    qualified_name: String,
    node_id: String,
    table: String,
    language: String,
    scope_id: String,
    visibility: String,
}

struct SemReference {
    graph_index: usize,
    reference_node_id: String,
    name: String,
    scope_id: String,
    language: String,
    source_path: String,
}

struct SemResolutionCandidate {
    target_node_id: String,
    score: f64,
    source: String,
    rationale: String,
}

struct SemEvidence {
    evidence_id: String,
    source: String,
    confidence: f64,
    diagnostics: Vec<String>,
    provider: String,
    metadata: BTreeMap<String, String>,
}

struct SemEvidenceLink {
    graph_index: usize,
    semantic_relation_id: String,
    evidence_node_id: String,
    evidence_kind: String,
    confidence: f64,
    metadata_fallback: bool,
}

struct SemFallback {
    graph_index: usize,
    semantic_relation_id: String,
    source_node_id: String,
    evidence_id: String,
    metadata: BTreeMap<String, String>,
}

struct SemanticOutput {
    symbol_count: usize,
    call_type_relations: usize,
    edges: Vec<SemEdge>,
    evidence: Vec<SemEvidence>,
    evidence_links: Vec<SemEvidenceLink>,
    fallbacks: Vec<SemFallback>,
}

fn run_semantic_batch(input: &str) -> Result<(), String> {
    let output = execute_semantic_batch(parse_semantic_batch(input)?)?;
    println!(
        "RESULT\t{}\t{}",
        output.symbol_count, output.call_type_relations
    );
    for edge in output.edges {
        println!("{}", encode_semantic_edge("EDGE", &edge));
    }
    for evidence in output.evidence {
        println!("{}", encode_semantic_evidence(&evidence));
    }
    for link in output.evidence_links {
        println!("{}", encode_semantic_link(&link));
    }
    for fallback in output.fallbacks {
        println!("{}", encode_semantic_fallback(&fallback));
    }
    Ok(())
}

fn parse_semantic_batch(input: &str) -> Result<SemanticInput, String> {
    let mut relation_specs = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    let mut node_order = Vec::new();
    let mut edges = BTreeMap::new();
    let mut edge_order = Vec::new();

    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("SEMANTIC") if parts.len() == 1 => {}
            Some("REL") if parts.len() >= 4 => {
                let mut cursor = 1;
                let name = next_unhex(&parts, &mut cursor, "relation name")?;
                let source_types = decode_hex_list_from_cursor(&parts, &mut cursor)?
                    .into_iter()
                    .collect();
                let target_types = decode_hex_list_from_cursor(&parts, &mut cursor)?
                    .into_iter()
                    .collect();
                relation_specs.insert(
                    name,
                    RelationSpec {
                        source_types,
                        target_types,
                    },
                );
            }
            Some("GRAPH") if parts.len() == 3 => {
                parts[1]
                    .parse::<usize>()
                    .map_err(|error| format!("invalid semantic graph index: {error}"))?;
                unhex(parts[2])?;
            }
            Some("SNODE") if parts.len() == 12 => {
                let node = SemNode {
                    graph_index: parts[1]
                        .parse::<usize>()
                        .map_err(|error| format!("invalid semantic node graph index: {error}"))?,
                    id: unhex(parts[2])?,
                    table: unhex(parts[3])?,
                    label: unhex(parts[4])?,
                    language: unhex(parts[5])?,
                    path: unhex(parts[6])?,
                    qualified_name: unhex(parts[7])?,
                    scope_id: unhex(parts[8])?,
                    imported_name: unhex(parts[9])?,
                    owner_node_id: unhex(parts[10])?,
                    typed_node_id: unhex(parts[11])?,
                };
                node_order.push(node.id.clone());
                nodes.insert(node.id.clone(), node);
            }
            Some("SEDGE") if parts.len() == 10 => {
                let edge = SemEdge {
                    graph_index: parts[1]
                        .parse::<usize>()
                        .map_err(|error| format!("invalid semantic edge graph index: {error}"))?,
                    id: unhex(parts[2])?,
                    edge_type: unhex(parts[3])?,
                    source_id: unhex(parts[4])?,
                    target_id: unhex(parts[5])?,
                    kind: unhex(parts[6])?,
                    confidence: parts[7]
                        .parse::<f64>()
                        .map_err(|error| format!("invalid semantic edge confidence: {error}"))?,
                    metadata: {
                        let mut metadata = BTreeMap::new();
                        let resolution_source = unhex(parts[8])?;
                        let source_edge = unhex(parts[9])?;
                        if !resolution_source.is_empty() {
                            metadata.insert("resolution_source".to_string(), resolution_source);
                        }
                        if !source_edge.is_empty() {
                            metadata.insert("source_edge".to_string(), source_edge);
                        }
                        metadata
                    },
                };
                edge_order.push(edge.id.clone());
                edges.insert(edge.id.clone(), edge);
            }
            Some(kind) => return Err(format!("invalid semantic batch record: {kind}")),
            None => {}
        }
    }

    Ok(SemanticInput {
        relation_specs,
        nodes,
        node_order,
        edges,
        edge_order,
    })
}

fn execute_semantic_batch(mut input: SemanticInput) -> Result<SemanticOutput, String> {
    let symbols = build_semantic_symbols(&input);
    let symbol_count = symbols.len();
    let by_name = index_symbols_by_name(&symbols);
    let mut output_edges = Vec::new();
    let mut evidence = Vec::new();

    for reference in collect_semantic_references(&input) {
        let Some(decision) = resolve_semantic_reference(&reference, &by_name) else {
            evidence.push(SemEvidence {
                evidence_id: semantic_stable_id(
                    "evidence",
                    &format!("unresolved:{}", reference.reference_node_id),
                ),
                source: "local".to_string(),
                confidence: 0.0,
                diagnostics: vec![format!("Unresolved reference: {}", reference.name)],
                provider: String::new(),
                metadata: BTreeMap::new(),
            });
            continue;
        };
        if let Some(primary) =
            semantic_resolves_to_edge(&mut input, &reference, &decision, &mut output_edges)
        {
            let mut metadata = BTreeMap::new();
            metadata.insert("edge_id".to_string(), primary.id.clone());
            metadata.insert(
                "target_node_id".to_string(),
                decision.target_node_id.clone(),
            );
            evidence.push(SemEvidence {
                evidence_id: semantic_stable_id("evidence", &primary.id),
                source: decision.source,
                confidence: decision.score,
                diagnostics: Vec::new(),
                provider: String::new(),
                metadata,
            });
        }
    }

    let call_type_relations =
        enrich_semantic_call_and_type_relations(&mut input, &mut output_edges);
    let (evidence_links, fallbacks) =
        build_semantic_evidence_links(&mut input, &evidence, &mut output_edges);

    Ok(SemanticOutput {
        symbol_count,
        call_type_relations,
        edges: output_edges,
        evidence,
        evidence_links,
        fallbacks,
    })
}

fn build_semantic_symbols(input: &SemanticInput) -> Vec<SemSymbol> {
    let exported_targets: BTreeSet<String> = input
        .edges
        .values()
        .filter(|edge| edge.edge_type == "Exports")
        .map(|edge| edge.target_id.clone())
        .collect();
    let mut symbols = Vec::new();
    for node_id in &input.node_order {
        let Some(node) = input.nodes.get(node_id) else {
            continue;
        };
        if !DECLARATION_TABLES.contains(&node.table.as_str()) {
            continue;
        }
        let name = node.label.trim();
        if name.is_empty() {
            continue;
        }
        let mut visibility = semantic_visibility(node);
        if exported_targets.contains(&node.id) {
            visibility = "exported".to_string();
        }
        symbols.push(SemSymbol {
            symbol_id: format!("{}:{}", node.table, node.id),
            name: name.to_string(),
            qualified_name: if node.qualified_name.is_empty() {
                name.to_string()
            } else {
                node.qualified_name.clone()
            },
            node_id: node.id.clone(),
            table: node.table.clone(),
            language: node.language.clone(),
            scope_id: node.scope_id.clone(),
            visibility,
        });
    }
    symbols.sort_by(|left, right| {
        (
            left.qualified_name.as_str(),
            left.table.as_str(),
            left.node_id.as_str(),
        )
            .cmp(&(
                right.qualified_name.as_str(),
                right.table.as_str(),
                right.node_id.as_str(),
            ))
    });
    symbols
}

fn index_symbols_by_name(symbols: &[SemSymbol]) -> BTreeMap<String, Vec<SemSymbol>> {
    let mut by_name: BTreeMap<String, Vec<SemSymbol>> = BTreeMap::new();
    for symbol in symbols {
        let _ = &symbol.symbol_id;
        for key in semantic_symbol_keys(&symbol.name, &symbol.qualified_name) {
            by_name.entry(key).or_default().push(symbol.clone());
        }
    }
    by_name
}

fn collect_semantic_references(input: &SemanticInput) -> Vec<SemReference> {
    let mut references = Vec::new();
    for node_id in &input.node_order {
        let Some(node) = input.nodes.get(node_id) else {
            continue;
        };
        if !REFERENCE_TABLES.contains(&node.table.as_str()) {
            continue;
        }
        let name = if node.imported_name.trim().is_empty() {
            node.label.trim().to_string()
        } else {
            node.imported_name.trim().to_string()
        };
        if name.is_empty() {
            continue;
        }
        references.push(SemReference {
            graph_index: node.graph_index,
            reference_node_id: node.id.clone(),
            name,
            scope_id: node.scope_id.clone(),
            language: node.language.clone(),
            source_path: node.path.clone(),
        });
    }
    references.sort_by(|left, right| {
        (left.source_path.as_str(), left.reference_node_id.as_str())
            .cmp(&(right.source_path.as_str(), right.reference_node_id.as_str()))
    });
    references
}

fn resolve_semantic_reference(
    reference: &SemReference,
    by_name: &BTreeMap<String, Vec<SemSymbol>>,
) -> Option<SemResolutionCandidate> {
    for key in candidate_semantic_symbol_keys(&reference.name) {
        let Some(candidates) = by_name.get(&key) else {
            continue;
        };
        let mut symbols = candidates.clone();
        symbols.sort_by(|left, right| {
            (
                left.scope_id != reference.scope_id,
                left.language != reference.language,
                !matches!(left.visibility.as_str(), "local" | "public" | "exported"),
                left.qualified_name.as_str(),
                left.node_id.as_str(),
            )
                .cmp(&(
                    right.scope_id != reference.scope_id,
                    right.language != reference.language,
                    !matches!(right.visibility.as_str(), "local" | "public" | "exported"),
                    right.qualified_name.as_str(),
                    right.node_id.as_str(),
                ))
        });
        let symbol = symbols.first()?;
        let mut score: f64 = 0.72;
        if symbol.scope_id == reference.scope_id {
            score += 0.13;
        }
        if symbol.language == reference.language {
            score += 0.05;
        }
        return Some(SemResolutionCandidate {
            target_node_id: symbol.node_id.clone(),
            score: score.min(1.0),
            source: "symbol_table".to_string(),
            rationale: format!("symbol_table matched {}", reference.name),
        });
    }
    None
}

fn semantic_resolves_to_edge(
    input: &mut SemanticInput,
    reference: &SemReference,
    candidate: &SemResolutionCandidate,
    output_edges: &mut Vec<SemEdge>,
) -> Option<SemEdge> {
    let source_id = input.nodes.get(&reference.reference_node_id)?.id.clone();
    let target_id = input.nodes.get(&candidate.target_node_id)?.id.clone();
    if source_id == target_id {
        return None;
    }
    let mut metadata = BTreeMap::new();
    metadata.insert("resolver".to_string(), "semantic".to_string());
    metadata.insert("resolution_source".to_string(), candidate.source.clone());
    metadata.insert("rationale".to_string(), candidate.rationale.clone());
    metadata.insert("label".to_string(), reference.name.clone());
    let primary = add_semantic_edge_if_allowed(
        input,
        output_edges,
        reference.graph_index,
        "ResolvesTo",
        &source_id,
        &target_id,
        "semantic_resolution",
        candidate.score,
        metadata.clone(),
    );
    add_semantic_edge_if_allowed(
        input,
        output_edges,
        reference.graph_index,
        "References",
        &source_id,
        &target_id,
        "semantic_reference",
        candidate.score.min(0.9),
        metadata,
    );
    primary
}

fn enrich_semantic_call_and_type_relations(
    input: &mut SemanticInput,
    output_edges: &mut Vec<SemEdge>,
) -> usize {
    let edge_ids = input.edge_order.clone();
    let mut resolutions = 0;
    for edge_id in edge_ids {
        let Some(edge) = input.edges.get(&edge_id).cloned() else {
            continue;
        };
        if edge.edge_type != "ResolvesTo" {
            continue;
        }
        let Some(source) = input.nodes.get(&edge.source_id).cloned() else {
            continue;
        };
        let Some(target) = input.nodes.get(&edge.target_id).cloned() else {
            continue;
        };
        if source.table == "CallExpression" {
            if !matches!(
                target.table.as_str(),
                "Function" | "Method" | "Class" | "APIEndpoint"
            ) {
                continue;
            }
            let mut metadata = BTreeMap::new();
            metadata.insert("resolver".to_string(), "semantic".to_string());
            metadata.insert("source_edge".to_string(), edge.id.clone());
            add_semantic_edge_if_allowed(
                input,
                output_edges,
                source.graph_index,
                "Calls",
                &source.id,
                &target.id,
                "semantic_call_target",
                edge.confidence,
                metadata,
            );
            resolutions += 1;
        } else if source.table == "TypeAnnotation" {
            let mut fallback_metadata = BTreeMap::new();
            fallback_metadata.insert("resolver".to_string(), "semantic".to_string());
            fallback_metadata.insert("source_edge".to_string(), edge.id.clone());
            add_semantic_edge_if_allowed(
                input,
                output_edges,
                source.graph_index,
                "References",
                &source.id,
                &target.id,
                "semantic_type_reference",
                edge.confidence,
                fallback_metadata.clone(),
            );
            if let Some(owner_id) = semantic_type_annotation_owner_id(input, &source.id) {
                let mut metadata = fallback_metadata;
                metadata.insert("target_node_id".to_string(), target.id.clone());
                add_semantic_edge_if_allowed(
                    input,
                    output_edges,
                    source.graph_index,
                    "HasTypeAnnotation",
                    &owner_id,
                    &source.id,
                    "semantic_type_annotation",
                    edge.confidence,
                    metadata,
                );
            }
            resolutions += 1;
        }
    }
    resolutions
}

fn semantic_type_annotation_owner_id(input: &SemanticInput, type_node_id: &str) -> Option<String> {
    let typed_owner_types = input
        .relation_specs
        .get("HasTypeAnnotation")
        .map(|spec| spec.source_types.clone())
        .unwrap_or_default();
    for edge_id in &input.edge_order {
        let Some(edge) = input.edges.get(edge_id) else {
            continue;
        };
        if edge.edge_type != "HasTypeAnnotation" || edge.target_id != type_node_id {
            continue;
        }
        let Some(owner) = input.nodes.get(&edge.source_id) else {
            continue;
        };
        if typed_owner_types.contains(&owner.table) {
            return Some(owner.id.clone());
        }
    }
    let type_node = input.nodes.get(type_node_id)?;
    if !type_node.scope_id.is_empty() {
        if let Some(owner) = input.nodes.get(&type_node.scope_id) {
            if typed_owner_types.contains(&owner.table) {
                return Some(owner.id.clone());
            }
        }
    }
    for owner_id in [&type_node.owner_node_id, &type_node.typed_node_id] {
        if owner_id.is_empty() {
            continue;
        }
        if let Some(owner) = input.nodes.get(owner_id) {
            if typed_owner_types.contains(&owner.table) {
                return Some(owner.id.clone());
            }
        }
    }
    None
}

fn build_semantic_evidence_links(
    input: &mut SemanticInput,
    evidence: &[SemEvidence],
    output_edges: &mut Vec<SemEdge>,
) -> (Vec<SemEvidenceLink>, Vec<SemFallback>) {
    let mut links = Vec::new();
    let mut fallbacks = Vec::new();
    for item in evidence {
        let semantic_relation_id = item.metadata.get("edge_id").cloned().unwrap_or_default();
        let Some(semantic_edge) = input.edges.get(&semantic_relation_id).cloned() else {
            continue;
        };
        let evidence_node_ids = semantic_evidence_node_ids(input, &semantic_edge.source_id, item);
        if evidence_node_ids.is_empty() {
            fallbacks.push(SemFallback {
                graph_index: semantic_edge.graph_index,
                semantic_relation_id,
                source_node_id: semantic_edge.source_id,
                evidence_id: item.evidence_id.clone(),
                metadata: item.metadata.clone(),
            });
            continue;
        }
        for evidence_node_id in evidence_node_ids {
            let Some(evidence_node) = input.nodes.get(&evidence_node_id).cloned() else {
                continue;
            };
            let mut metadata = BTreeMap::new();
            metadata.insert("resolver".to_string(), "semantic".to_string());
            metadata.insert(
                "semantic_relation_id".to_string(),
                semantic_relation_id.clone(),
            );
            metadata.insert("evidence_id".to_string(), item.evidence_id.clone());
            metadata.insert("source".to_string(), item.source.clone());
            metadata.insert("provider".to_string(), item.provider.clone());
            let edge = semantic_evidence_edge_if_allowed(
                input,
                output_edges,
                semantic_edge.graph_index,
                &semantic_edge.source_id,
                &evidence_node_id,
                item.confidence,
                metadata,
            );
            if edge.is_none() {
                continue;
            }
            links.push(SemEvidenceLink {
                graph_index: semantic_edge.graph_index,
                semantic_relation_id: semantic_relation_id.clone(),
                evidence_node_id,
                evidence_kind: evidence_node.table,
                confidence: item.confidence,
                metadata_fallback: false,
            });
        }
    }
    (links, fallbacks)
}

fn semantic_evidence_node_ids(
    input: &SemanticInput,
    source_node_id: &str,
    evidence: &SemEvidence,
) -> Vec<String> {
    let mut node_ids = Vec::new();
    if let Some(explicit_id) = evidence.metadata.get("evidence_node_id") {
        if semantic_is_valid_evidence_target(input, explicit_id) {
            node_ids.push(explicit_id.clone());
        }
    }
    for edge_id in &input.edge_order {
        let Some(edge) = input.edges.get(edge_id) else {
            continue;
        };
        if edge.edge_type == "DerivedFrom"
            && edge.source_id == source_node_id
            && semantic_is_valid_evidence_target(input, &edge.target_id)
        {
            node_ids.push(edge.target_id.clone());
        }
    }
    if let Some(source_node) = input.nodes.get(source_node_id) {
        if !source_node.path.is_empty() {
            for node_id in &input.node_order {
                let Some(node) = input.nodes.get(node_id) else {
                    continue;
                };
                if node.table == "File"
                    && node.path == source_node.path
                    && semantic_is_valid_evidence_target(input, &node.id)
                {
                    node_ids.push(node.id.clone());
                }
            }
        }
    }
    dedupe_strings(node_ids)
}

fn semantic_is_valid_evidence_target(input: &SemanticInput, node_id: &str) -> bool {
    input.nodes.get(node_id).is_some_and(|node| {
        matches!(
            node.table.as_str(),
            "SyntaxCapture" | "File" | "DocumentationChunk"
        )
    })
}

fn semantic_evidence_edge_if_allowed(
    input: &mut SemanticInput,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    source_id: &str,
    target_id: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let edge_id = format!(
        "edge:semantic-evidence:{}",
        sha1_hex(
            format!(
                "{}|{}|{}",
                source_id,
                target_id,
                metadata
                    .get("semantic_relation_id")
                    .cloned()
                    .unwrap_or_default()
            )
            .as_bytes()
        )
        .chars()
        .take(20)
        .collect::<String>()
    );
    semantic_edge_if_allowed_with_id(
        input,
        output_edges,
        graph_index,
        edge_id,
        "EvidencedBy",
        source_id,
        target_id,
        "semantic_evidence",
        confidence,
        metadata,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_semantic_edge_if_allowed(
    input: &mut SemanticInput,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    edge_type: &str,
    source_id: &str,
    target_id: &str,
    kind: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let edge_id = semantic_stable_id(
        "edge",
        &format!("{edge_type}|{source_id}|{target_id}|{kind}"),
    );
    semantic_edge_if_allowed_with_id(
        input,
        output_edges,
        graph_index,
        edge_id,
        edge_type,
        source_id,
        target_id,
        kind,
        confidence,
        metadata,
    )
}

#[allow(clippy::too_many_arguments)]
fn semantic_edge_if_allowed_with_id(
    input: &mut SemanticInput,
    output_edges: &mut Vec<SemEdge>,
    graph_index: usize,
    edge_id: String,
    edge_type: &str,
    source_id: &str,
    target_id: &str,
    kind: &str,
    confidence: f64,
    metadata: BTreeMap<String, String>,
) -> Option<SemEdge> {
    let source = input.nodes.get(source_id)?;
    let target = input.nodes.get(target_id)?;
    let spec = input.relation_specs.get(edge_type)?;
    if !spec.source_types.contains(&source.table) || !spec.target_types.contains(&target.table) {
        return None;
    }
    let mut full_metadata = BTreeMap::new();
    full_metadata.insert(
        "canonical_key".to_string(),
        format!("{edge_type}|{source_id}|{target_id}|{kind}"),
    );
    for (key, value) in metadata {
        full_metadata.insert(key, value);
    }
    let edge = SemEdge {
        graph_index,
        id: edge_id.clone(),
        edge_type: edge_type.to_string(),
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        kind: kind.to_string(),
        confidence,
        metadata: full_metadata,
    };
    if !input.edges.contains_key(&edge_id) {
        input.edge_order.push(edge_id.clone());
        input.edges.insert(edge_id, edge.clone());
        output_edges.push(edge.clone());
    }
    Some(input.edges.get(&edge.id).cloned().unwrap_or(edge))
}

fn semantic_symbol_keys(name: &str, qualified_name: &str) -> Vec<String> {
    let mut keys: BTreeSet<String> = candidate_semantic_symbol_keys(name).into_iter().collect();
    keys.extend(candidate_semantic_symbol_keys(qualified_name));
    keys.into_iter().collect()
}

fn candidate_semantic_symbol_keys(label: &str) -> Vec<String> {
    let text = label.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let mut parts = BTreeSet::new();
    parts.insert(text.to_string());
    for delimiter in [".", "::", "->"] {
        if text.contains(delimiter) {
            if let Some((_, right)) = text.rsplit_once(delimiter) {
                parts.insert(right.to_string());
            }
        }
    }
    if text.contains('/') {
        if let Some((_, right)) = text.rsplit_once('/') {
            parts.insert(right.to_string());
        }
    }
    parts
        .into_iter()
        .filter_map(|part| {
            let normalized = part.trim().to_lowercase().replace('_', "");
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn semantic_visibility(node: &SemNode) -> String {
    if node.table == "Dependency" {
        "external".to_string()
    } else if node.label.starts_with('_') {
        "private".to_string()
    } else if node.label.chars().next().is_some_and(char::is_uppercase)
        || matches!(
            node.table.as_str(),
            "Module" | "Class" | "Function" | "Method" | "TypeAlias"
        )
    {
        "public".to_string()
    } else {
        "local".to_string()
    }
}

fn semantic_stable_id(prefix: &str, key: &str) -> String {
    format!("{prefix}:{}", sha1_hex(key.as_bytes()))
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            output.push(value);
        }
    }
    output
}

fn encode_semantic_edge(record_type: &str, edge: &SemEdge) -> String {
    encode_record(&[
        record_type,
        &edge.graph_index.to_string(),
        &hex(&edge.id),
        &hex(&edge.edge_type),
        &hex(&edge.source_id),
        &hex(&edge.target_id),
        &hex(&edge.kind),
        &edge.confidence.to_string(),
        &hex(&json_string_object(&edge.metadata)),
    ])
}

fn encode_semantic_evidence(evidence: &SemEvidence) -> String {
    let diagnostics = evidence
        .diagnostics
        .iter()
        .map(|diagnostic| json_string(diagnostic))
        .collect::<Vec<_>>()
        .join(",");
    encode_record(&[
        "EVIDENCE",
        &hex(&evidence.evidence_id),
        &hex(&evidence.source),
        &evidence.confidence.to_string(),
        &hex(&evidence.provider),
        &hex(&format!("[{diagnostics}]")),
        &hex(&json_string_object(&evidence.metadata)),
    ])
}

fn encode_semantic_link(link: &SemEvidenceLink) -> String {
    encode_record(&[
        "LINK",
        &link.graph_index.to_string(),
        &hex(&link.semantic_relation_id),
        &hex(&link.evidence_node_id),
        &hex(&link.evidence_kind),
        &link.confidence.to_string(),
        if link.metadata_fallback { "1" } else { "0" },
    ])
}

fn encode_semantic_fallback(fallback: &SemFallback) -> String {
    encode_record(&[
        "FALLBACK",
        &fallback.graph_index.to_string(),
        &hex(&fallback.semantic_relation_id),
        &hex(&fallback.source_node_id),
        &hex(&fallback.evidence_id),
        &hex(&json_string_object(&fallback.metadata)),
    ])
}

fn json_string_object(values: &BTreeMap<String, String>) -> String {
    let fields = values
        .iter()
        .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn parse_input(input: &str) -> Result<(BTreeMap<String, String>, Vec<Capture>), String> {
    let mut meta = BTreeMap::new();
    let mut captures = Vec::new();
    for line in input.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("META") if parts.len() == 3 => {
                meta.insert(parts[1].to_string(), unhex(parts[2])?);
            }
            Some("CAP") if parts.len() == 10 => {
                let fields = unhex(parts[9])?
                    .split(',')
                    .filter(|field| !field.is_empty())
                    .map(str::to_string)
                    .collect();
                captures.push(Capture {
                    capture_name: unhex(parts[1])?,
                    node_type: unhex(parts[2])?,
                    label: unhex(parts[3])?,
                    text: unhex(parts[4])?,
                    line_start: parse_optional_i64(parts[5])?,
                    line_end: parse_optional_i64(parts[6])?,
                    byte_start: parse_optional_i64(parts[7])?,
                    byte_end: parse_optional_i64(parts[8])?,
                    fields,
                });
            }
            Some(kind) => return Err(format!("invalid input record: {}", kind)),
            None => {}
        }
    }
    Ok((meta, captures))
}

#[allow(dead_code)]
fn parse_tree_graph_input(input: &str) -> Result<TreeGraphInput, String> {
    let mut meta = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    let mut root_id = None;
    for line in input.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.first().copied() {
            Some("TREEGRAPH") if parts.len() == 1 => {}
            Some("META") if parts.len() == 3 => {
                meta.insert(parts[1].to_string(), unhex(parts[2])?);
            }
            Some("NODE") if parts.len() >= 11 => {
                let node = decode_tree_graph_node(&parts[1..])?;
                if node.parent_id.is_none() {
                    root_id = Some(node.id);
                }
                nodes.insert(node.id, node);
            }
            Some(kind) => return Err(format!("invalid tree graph record: {kind}")),
            None => {}
        }
    }
    for node_id in nodes.keys().copied().collect::<Vec<_>>() {
        let Some(parent_id) = nodes.get(&node_id).and_then(|node| node.parent_id) else {
            continue;
        };
        let Some(parent) = nodes.get_mut(&parent_id) else {
            return Err(format!(
                "tree graph node {node_id} references missing parent {parent_id}"
            ));
        };
        parent.children.push(node_id);
    }
    Ok(TreeGraphInput {
        meta,
        nodes,
        root_id: root_id.unwrap_or(0),
    })
}

fn append_syntax_tree_node(
    node: &SyntaxNode,
    parent_id: Option<usize>,
    next_id: &mut usize,
    nodes: &mut BTreeMap<usize, TsNode>,
) -> usize {
    let node_id = *next_id;
    *next_id += 1;
    let children = node
        .children
        .iter()
        .map(|child| append_syntax_tree_node(child, Some(node_id), next_id, nodes))
        .collect();
    nodes.insert(
        node_id,
        TsNode {
            id: node_id,
            parent_id,
            node_type: node.node_type.clone(),
            text: node.text.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            byte_start: node.byte_start,
            byte_end: node.byte_end,
            capture_name: node.capture_name.clone(),
            fields: syntax_node_fields(&node.fields),
            field_types: BTreeMap::new(),
            field_descendant_types: BTreeMap::new(),
            children,
        },
    );
    node_id
}

fn syntax_node_fields(fields: &BTreeMap<String, serde_json::Value>) -> BulkRow {
    fields
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
            )
        })
        .collect()
}

#[allow(dead_code)]
fn decode_tree_graph_node(parts: &[&str]) -> Result<TsNode, String> {
    let id = parts[0]
        .parse::<usize>()
        .map_err(|error| format!("invalid tree graph node id: {error}"))?;
    let parent_id = if parts[1].is_empty() {
        None
    } else {
        Some(
            parts[1]
                .parse::<usize>()
                .map_err(|error| format!("invalid tree graph parent id: {error}"))?,
        )
    };
    let node_type = unhex(parts[2])?;
    let text = unhex(parts[3])?;
    let line_start = parse_optional_i64(parts[4])?;
    let line_end = parse_optional_i64(parts[5])?;
    let byte_start = parse_optional_i64(parts[6])?;
    let byte_end = parse_optional_i64(parts[7])?;
    let capture_name = unhex(parts[8])?;
    let mut cursor = 9;
    let fields = decode_ts_field_tokens(parts, &mut cursor)?;
    if cursor != parts.len() {
        return Err("tree graph node has trailing fields".to_string());
    }
    Ok(TsNode {
        id,
        parent_id,
        node_type,
        text,
        line_start,
        line_end,
        byte_start,
        byte_end,
        capture_name,
        fields,
        field_types: BTreeMap::new(),
        field_descendant_types: BTreeMap::new(),
        children: Vec::new(),
    })
}

fn tree_capture(node: &TsNode) -> Capture {
    Capture {
        capture_name: node.capture_name.clone(),
        node_type: node.node_type.clone(),
        label: tree_label(node),
        text: node.text.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        fields: node.fields.keys().cloned().collect(),
    }
}

fn tree_label(node: &TsNode) -> String {
    for key in ["name", "id", "arg", "attr", "module", "path", "function"] {
        if let Some(label) = node
            .fields
            .get(key)
            .and_then(|value| json_token_label(value))
        {
            if !label.is_empty() {
                return label;
            }
        }
    }
    if let Some(label) = node
        .fields
        .get("value")
        .and_then(|value| json_token_label(value))
    {
        if !label.is_empty() {
            return label;
        }
    }
    let text = node.text.trim();
    if text.is_empty() {
        node.node_type.clone()
    } else {
        text.to_string()
    }
}

fn should_derive_root_module(language: &str, root_node_type: &str) -> bool {
    !(matches!(root_node_type, "source_file")
        || (language == "python" && root_node_type == "module")
        || (language == "markdown" && root_node_type == "Module"))
}

fn json_token_label(token: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(token).ok()?;
    match value {
        serde_json::Value::String(text) => Some(text.trim().to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(boolean) => Some(boolean.to_string()),
        serde_json::Value::Object(object) => {
            for key in ["id", "name", "arg", "attr", "value"] {
                if let Some(value) = object.get(key) {
                    let encoded = serde_json::to_string(value).ok()?;
                    let label = json_token_label(&encoded)?;
                    if !label.is_empty() {
                        return Some(label);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn semantic_child_ids(
    nodes: &BTreeMap<usize, TsNode>,
    parent: &TsNode,
    language: &str,
) -> Vec<usize> {
    let mut child_ids = Vec::new();
    for child_id in &parent.children {
        let Some(child) = nodes.get(child_id) else {
            continue;
        };
        if should_inline_child(parent, child, language) {
            child_ids.extend(semantic_child_ids(nodes, child, language));
        } else if should_traverse_child(parent, child, language) {
            child_ids.push(*child_id);
        }
    }
    child_ids
}

fn should_inline_child(_parent: &TsNode, child: &TsNode, language: &str) -> bool {
    (language == "python" && child.node_type == "block")
        || (language == "fortran" && child.node_type == "variable_declaration")
}

fn should_traverse_child(parent: &TsNode, child: &TsNode, language: &str) -> bool {
    if language == "python" {
        if parent.node_type == "attribute" {
            return json_field_label(parent, "value")
                .is_some_and(|label| label == tree_label(child));
        }
        if matches!(child.node_type.as_str(), "parameters" | "decorator") {
            return false;
        }
        if matches!(
            parent.node_type.as_str(),
            "class_definition" | "function_definition"
        ) && matches!(
            child.node_type.as_str(),
            "identifier" | "type" | "type_identifier"
        ) {
            return false;
        }
        if matches!(child.node_type.as_str(), "identifier" | "type_identifier")
            && !matches!(
                parent.node_type.as_str(),
                "assignment" | "call" | "attribute"
            )
        {
            return false;
        }
    }
    if child.node_type == "block" {
        return true;
    }
    if matches!(
        parent.node_type.as_str(),
        "import_statement" | "import_from_statement" | "import_declaration" | "use_declaration"
    ) && matches!(
        child.node_type.as_str(),
        "identifier"
            | "dotted_name"
            | "aliased_import"
            | "import_list"
            | "string"
            | "interpreted_string_literal"
            | "raw_string_literal"
            | "string_literal"
    ) {
        if language == "python" && child.node_type == "dotted_name" {
            return true;
        }
        return false;
    }
    true
}

fn table_for_node_type(node_type: &str, owner: &Owner) -> Option<String> {
    Some(
        match node_type {
            "import_statement"
            | "import_from_statement"
            | "import_declaration"
            | "use_declaration"
            | "preproc_include"
            | "use_statement" => "ImportDeclaration",
            "export_statement" | "export_clause" | "export_declaration" => "ExportDeclaration",
            "class_definition"
            | "class_declaration"
            | "struct_item"
            | "interface_declaration"
            | "struct_specifier"
            | "union_specifier"
            | "enum_specifier"
            | "class_specifier"
            | "type_declaration" => "Class",
            "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "function_item"
            | "subroutine"
            | "function" => {
                if matches!(owner.table.as_str(), "Class" | "Component") {
                    "Method"
                } else {
                    "Function"
                }
            }
            "arg" => "Parameter",
            "return_type" | "returns" => "ReturnType",
            "type" | "type_identifier" | "qualified_type" | "type_annotation" | "annotation" => {
                "TypeAnnotation"
            }
            "assignment" | "assignment_expression" => "Assignment",
            "call"
            | "call_expression"
            | "invocation_expression"
            | "call_statement"
            | "subroutine_call" => "CallExpression",
            "identifier" | "field_identifier" | "attribute" => "Reference",
            "string" | "integer" | "float" | "true" | "false" | "null" | "none"
            | "intrinsic_type" => "Literal",
            "if_statement" | "for_statement" | "while_statement" | "match_statement"
            | "switch_statement" => "ControlFlowBlock",
            "try_statement" | "except_clause" | "catch_clause" | "raise_statement"
            | "throw_statement" => "ExceptionFlow",
            _ => return None,
        }
        .to_string(),
    )
}

fn json_field_label(node: &TsNode, field: &str) -> Option<String> {
    node.fields
        .get(field)
        .and_then(|value| json_token_label(value))
}

fn import_label(node: &TsNode) -> Option<String> {
    let module = json_field_label(node, "module").unwrap_or_default();
    let names = node
        .fields
        .get("names")
        .and_then(|token| serde_json::from_str::<serde_json::Value>(token).ok())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| {
            serde_json::to_string(&item)
                .ok()
                .and_then(|token| json_token_label(&token))
        })
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if !module.is_empty() && !names.is_empty() {
        Some(
            names
                .into_iter()
                .map(|name| format!("{module}.{name}"))
                .collect::<Vec<_>>()
                .join(", "),
        )
    } else if !module.is_empty() {
        Some(module)
    } else if !names.is_empty() {
        Some(names.join(", "))
    } else {
        None
    }
}

fn assignment_target_label(nodes: &BTreeMap<usize, TsNode>, node: &TsNode) -> Option<String> {
    json_field_label(node, "target").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get(child_id))
            .find(|child| !matches!(child.node_type.as_str(), "call" | "call_expression"))
            .map(tree_label)
            .filter(|label| !label.is_empty())
    })
}

fn assignment_target_table(label: &str, owner: &Owner, node_type: &str) -> &'static str {
    if label.chars().any(|character| character.is_alphabetic())
        && label
            .chars()
            .filter(|character| character.is_alphabetic())
            .all(|character| character.is_uppercase())
    {
        return "Constant";
    }
    if owner.table == "Class" {
        return "ClassAttribute";
    }
    if label.contains('.') {
        return "InstanceAttribute";
    }
    if node_type == "AnnAssign" && owner.table == "Class" {
        return "ClassAttribute";
    }
    "Variable"
}

fn call_value_child(nodes: &BTreeMap<usize, TsNode>, node: &TsNode) -> Option<usize> {
    node.children.iter().copied().find(|child_id| {
        nodes
            .get(child_id)
            .is_some_and(|child| matches!(child.node_type.as_str(), "call" | "call_expression"))
    })
}

fn parameter_child_ids(nodes: &BTreeMap<usize, TsNode>, function_node: &TsNode) -> Vec<usize> {
    function_node
        .children
        .iter()
        .filter_map(|child_id| nodes.get(child_id))
        .filter(|child| child.node_type == "parameters")
        .flat_map(|parameters| parameters.children.iter().copied())
        .filter(|child_id| {
            nodes.get(child_id).is_some_and(|child| {
                matches!(
                    child.node_type.as_str(),
                    "identifier" | "typed_parameter" | "default_parameter" | "parameter"
                )
            })
        })
        .collect()
}

fn parameter_label(node: &TsNode) -> String {
    let label = tree_label(node);
    label
        .split_once(':')
        .map(|(left, _)| left)
        .unwrap_or(label.as_str())
        .split_once('=')
        .map(|(left, _)| left)
        .unwrap_or_else(|| {
            label
                .split_once(':')
                .map(|(left, _)| left)
                .unwrap_or(label.as_str())
        })
        .trim()
        .trim_start_matches('*')
        .to_string()
}

fn parameter_capture(node: &TsNode, language: &str) -> Capture {
    if language != "python" {
        let mut capture = tree_capture(node);
        capture.capture_name = "parameter".to_string();
        capture.label = parameter_label(node);
        return capture;
    }
    Capture {
        capture_name: String::new(),
        node_type: "arg".to_string(),
        label: parameter_label(node),
        text: node.text.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        fields: vec!["arg".to_string()],
    }
}

fn parameter_annotation_label(nodes: &BTreeMap<usize, TsNode>, node: &TsNode) -> Option<String> {
    json_field_label(node, "annotation").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get(child_id))
            .find(|child| {
                matches!(
                    child.node_type.as_str(),
                    "type" | "type_identifier" | "qualified_type" | "annotation"
                )
            })
            .map(tree_label)
            .filter(|label| !label.is_empty())
    })
}

fn parameter_annotation_capture(
    nodes: &BTreeMap<usize, TsNode>,
    node: &TsNode,
    language: &str,
) -> Option<Capture> {
    if language == "python" {
        if let Some(type_child) = first_child_with_type(nodes, node, &["type", "type_identifier"]) {
            return Some(tree_capture(type_child));
        }
        return json_field_label(node, "annotation").map(|label| Capture {
            capture_name: String::new(),
            node_type: "type".to_string(),
            label,
            text: node.text.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
            byte_start: node.byte_start,
            byte_end: node.byte_end,
            fields: vec!["id".to_string()],
        });
    }
    parameter_annotation_label(nodes, node).map(|label| Capture {
        capture_name: String::new(),
        node_type: "type_annotation".to_string(),
        label,
        text: node.text.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        fields: Vec::new(),
    })
}

fn return_type_capture(
    nodes: &BTreeMap<usize, TsNode>,
    function_node: &TsNode,
    language: &str,
) -> Option<Capture> {
    if language == "python" {
        return first_child_with_type(nodes, function_node, &["type", "type_identifier"])
            .map(tree_capture);
    }
    json_field_label(function_node, "return_type")
        .or_else(|| json_field_label(function_node, "returns"))
        .map(|label| Capture {
            capture_name: "return_type".to_string(),
            node_type: "return_type".to_string(),
            label,
            text: function_node.text.clone(),
            line_start: function_node.line_start,
            line_end: function_node.line_end,
            byte_start: function_node.byte_start,
            byte_end: function_node.byte_end,
            fields: Vec::new(),
        })
}

fn first_child_with_type<'a>(
    nodes: &'a BTreeMap<usize, TsNode>,
    node: &TsNode,
    node_types: &[&str],
) -> Option<&'a TsNode> {
    node.children.iter().find_map(|child_id| {
        let child = nodes.get(child_id)?;
        if node_types
            .iter()
            .any(|node_type| child.node_type == *node_type)
        {
            Some(child)
        } else {
            None
        }
    })
}

fn fortran_literal_capture(nodes: &BTreeMap<usize, TsNode>, node: &TsNode) -> Option<Capture> {
    if node.node_type != "intrinsic_type" {
        return None;
    }
    let parent = node.parent_id.and_then(|parent_id| nodes.get(&parent_id))?;
    if parent.node_type != "variable_declaration" {
        return None;
    }
    Some(Capture {
        capture_name: String::new(),
        node_type: "integer".to_string(),
        label: parent.text.clone(),
        text: parent.text.clone(),
        line_start: node.line_start,
        line_end: node.line_end,
        byte_start: node.byte_start,
        byte_end: node.byte_end,
        fields: Vec::new(),
    })
}

fn parser_like_metadata_capture(node: &TsNode, field_name: &str) -> Option<Capture> {
    let value = node.fields.get(field_name)?;
    let metadata = serde_json::from_str::<serde_json::Value>(value).ok()?;
    let object = metadata.as_object()?;
    let node_type_value = object.get("type")?;
    let node_type = metadata_value_label(node_type_value)?;
    if node_type.is_empty() {
        return None;
    }
    let label = metadata_object_label(object).unwrap_or_else(|| node_type.clone());
    Some(Capture {
        capture_name: String::new(),
        node_type,
        text: label.clone(),
        label,
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        fields: object.keys().cloned().collect(),
    })
}

fn metadata_object_label(object: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    for key in ["name", "id", "arg", "attr", "module"] {
        if let Some(label) = object.get(key).and_then(metadata_value_label) {
            if !label.is_empty() {
                return Some(label);
            }
        }
    }
    if let Some(label) = object.get("value").and_then(metadata_value_label) {
        if !label.is_empty() {
            return Some(label);
        }
    }
    for key in [
        "name",
        "module",
        "path",
        "function",
        "type",
        "return_type",
        "declarator",
    ] {
        if let Some(label) = object.get(key).and_then(metadata_value_label) {
            if !label.is_empty() {
                return Some(label);
            }
        }
    }
    None
}

fn metadata_value_label(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(boolean) => Some(boolean.to_string()),
        serde_json::Value::Array(items) => Some(format!(
            "[{}]",
            items
                .iter()
                .filter_map(metadata_value_label)
                .map(|item| format!("'{item}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        serde_json::Value::Object(object) => metadata_object_label(object),
        serde_json::Value::Null => None,
    }
}

fn table_for_capture(capture: &str, owner: &Owner) -> Option<String> {
    let normalized = capture.trim_start_matches('@');
    Some(
        match normalized {
            "definition.class"
            | "definition.struct"
            | "definition.interface"
            | "definition.enum"
            | "definition.union" => "Class",
            "definition.module" | "definition.namespace" | "definition.package" => "Module",
            "definition.component" | "component" => "Component",
            "definition.method" => "Method",
            "definition.function" => {
                if matches!(owner.table.as_str(), "Class" | "Component") {
                    "Method"
                } else {
                    "Function"
                }
            }
            "definition.parameter" | "parameter" => "Parameter",
            "type.return" | "return_type" => "ReturnType",
            "type" | "type.annotation" | "reference.type" => "TypeAnnotation",
            "definition.type_alias" => "TypeAlias",
            "definition.macro" => "Symbol",
            "definition.constant" => "Constant",
            "definition.variable" => "Variable",
            "decorator" | "definition.decorator" => "Decorator",
            "reference.import" | "reference.include" | "reference.require" | "reference.use"
            | "import" => "ImportDeclaration",
            "export" | "definition.export" => "ExportDeclaration",
            "reference.call" | "call" => "CallExpression",
            "entrypoint.api" => "APIEndpoint",
            "endpoint" => "APIEndpoint",
            "route" => "Route",
            "doc.source" => "DocumentationSource",
            "literal" | "string" | "number" => "Literal",
            "control_flow" => "ControlFlowBlock",
            "exception" | "raises" | "handles" => "ExceptionFlow",
            value if value.starts_with("query.") => "Query",
            value if value.starts_with("secret.") => "SecretRef",
            value if value.starts_with("doc") => "DocumentationChunk",
            value if value.starts_with("reference") => "Reference",
            _ => return None,
        }
        .to_string(),
    )
}

fn legacy_relation_allowed(edge_type: &str, source: &str, target: &str) -> bool {
    match edge_type {
        "Imports" => {
            matches!(source, "File" | "Module" | "Scope")
                && matches!(
                    target,
                    "ImportDeclaration" | "Dependency" | "Module" | "Symbol"
                )
        }
        "References" => {
            matches!(
                source,
                "Reference"
                    | "Expression"
                    | "CallExpression"
                    | "Assignment"
                    | "ControlFlowBlock"
                    | "TypeAnnotation"
                    | "Decorator"
                    | "Query"
                    | "SecretRef"
            ) && (is_symbol_target(target) || matches!(target, "Module" | "Dependency"))
        }
        "Calls" => {
            matches!(
                source,
                "Function"
                    | "Method"
                    | "CallExpression"
                    | "Decorator"
                    | "APIEndpoint"
                    | "Route"
                    | "Component"
            ) && matches!(
                target,
                "CallExpression" | "Function" | "Method" | "Class" | "APIEndpoint"
            )
        }
        "ResolvesTo" => {
            matches!(
                source,
                "Reference"
                    | "ImportDeclaration"
                    | "CallExpression"
                    | "TypeAnnotation"
                    | "Decorator"
            ) && (is_symbol_target(target) || matches!(target, "Module" | "Dependency"))
        }
        "Documents" => {
            matches!(
                source,
                "DocumentationSource" | "DocumentationChunk" | "Literal"
            ) && (matches!(target, "Repository" | "File" | "Module") || is_declaration(target))
        }
        "EvidencedBy" => {
            (matches!(source, "Repository" | "File" | "Module" | "Dependency")
                || is_declaration(source)
                || is_expression(source)
                || is_documentation(source))
                && matches!(target, "SyntaxCapture" | "File" | "DocumentationChunk")
        }
        _ => true,
    }
}

fn is_symbol_target(table: &str) -> bool {
    matches!(
        table,
        "Symbol"
            | "Class"
            | "Function"
            | "Method"
            | "Variable"
            | "Constant"
            | "ClassAttribute"
            | "InstanceAttribute"
            | "Property"
            | "Parameter"
    )
}

fn is_declaration(table: &str) -> bool {
    matches!(
        table,
        "Symbol"
            | "Class"
            | "Function"
            | "Method"
            | "Parameter"
            | "ReturnType"
            | "TypeAnnotation"
            | "TypeAlias"
            | "Variable"
            | "Constant"
            | "ClassAttribute"
            | "InstanceAttribute"
            | "Property"
            | "Decorator"
            | "Assignment"
            | "APIEndpoint"
            | "Component"
            | "Route"
            | "Query"
            | "SecretRef"
    )
}

fn is_expression(table: &str) -> bool {
    matches!(
        table,
        "CallExpression"
            | "Assignment"
            | "Reference"
            | "Literal"
            | "Expression"
            | "ControlFlowBlock"
            | "ExceptionFlow"
            | "Query"
            | "SecretRef"
    )
}

fn is_documentation(table: &str) -> bool {
    matches!(table, "DocumentationSource" | "DocumentationChunk")
}

fn module_label(path: &str) -> String {
    let stem = path.rsplit_once('.').map(|(left, _)| left).unwrap_or(path);
    stem.replace('/', ".")
}

fn qualified_name(owner: &str, label: &str) -> String {
    if owner.is_empty() || owner == label {
        label.to_string()
    } else if label.is_empty() {
        owner.to_string()
    } else {
        format!("{}.{}", owner, label)
    }
}

fn kind_for(table: &str, node_type: &str) -> String {
    match table {
        "Method" => "method".to_string(),
        "Function" => "function".to_string(),
        "Class" => "class".to_string(),
        _ => node_type.to_string(),
    }
}

fn imported_name(node: &Node) -> String {
    match node.metadata.get("imported_name") {
        Some(JsonValue::String(value)) => value.clone(),
        _ => String::new(),
    }
}

fn symbol_key(label: &str) -> String {
    label.trim().to_lowercase()
}

fn graph_id(prefix: &str, value: &str) -> String {
    format!("{}:{}", prefix, sha1_hex(value.as_bytes()))
}

pub(crate) fn stable_partition_id(path: &str) -> String {
    sha1_hex(path.as_bytes())
}

pub(crate) fn stable_sha256_hex(input: &[u8]) -> String {
    sha256_hex(input)
}

fn sha1_hex(bytes: &[u8]) -> String {
    let digest = sha1(bytes);
    digest[..10]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

fn sha1(input: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xefcdab89;
    let mut h2: u32 = 0x98badcfe;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xc3d2e1f0;

    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut words = [0u32; 80];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut output = [0u8; 20];
    for (index, word) in [h0, h1, h2, h3, h4].iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_hex(input: &[u8]) -> String {
    let digest = sha256(input);
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn sha256(input: &[u8]) -> [u8; 32] {
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        for (index, value) in [a, b, c, d, e, f, g, hh].iter().enumerate() {
            h[index] = h[index].wrapping_add(*value);
        }
    }

    let mut output = [0u8; 32];
    for (index, word) in h.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn json_object(values: &BTreeMap<String, JsonValue>) -> String {
    let fields: Vec<String> = values
        .iter()
        .map(|(key, value)| format!("{}:{}", json_string(key), json_value(value)))
        .collect();
    format!("{{{}}}", fields.join(","))
}

fn json_value(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => json_string(text),
        JsonValue::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(json_value)
                .collect::<Vec<String>>()
                .join(",")
        ),
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            value if value.is_control() => escaped.push_str(&format!("\\u{:04x}", value as u32)),
            value => escaped.push(value),
        }
    }
    escaped.push('"');
    escaped
}

fn hex_json_string(value: &str) -> String {
    hex(&json_string(value))
}

fn encode_record(fields: &[&str]) -> String {
    fields.join("\t")
}

fn hex(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

fn unhex(value: &str) -> Result<String, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hex input has odd length".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        let byte =
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|error| error.to_string())?;
        bytes.push(byte);
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

fn optional_i64(value: Option<i64>) -> String {
    value.map(|item| item.to_string()).unwrap_or_default()
}

fn stable_optional_i64(value: Option<i64>) -> String {
    value
        .map(|item| item.to_string())
        .unwrap_or_else(|| "None".to_string())
}

fn parse_optional_i64(value: &str) -> Result<Option<i64>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        value
            .parse::<i64>()
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha1_matches_python_builder_prefix() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e");
    }

    #[test]
    fn graph_id_uses_twenty_sha1_bytes() {
        assert_eq!(
            graph_id("edge", "Contains|a|b|kind"),
            "edge:38fc26596ca334d0120d"
        );
    }

    #[test]
    fn sha256_matches_python_hashlib() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn relation_allowlist_uses_supplied_ontology_pairs() {
        let mut meta = BTreeMap::new();
        meta.insert(
            "ontology_relations".to_string(),
            r#"[{"name":"References","source_types":["Reference"],"target_types":["Symbol"]}]"#
                .to_string(),
        );

        let allowlist = RelationAllowlist::from_meta(&meta).unwrap();

        assert!(allowlist.allows("References", "Reference", "Symbol"));
        assert!(!allowlist.allows("References", "File", "Symbol"));
        assert!(!allowlist.allows("Unknown", "Reference", "Symbol"));
    }

    #[test]
    fn relation_allowlist_falls_back_without_ontology_metadata() {
        let allowlist = RelationAllowlist::from_meta(&BTreeMap::new()).unwrap();

        assert!(allowlist.allows("Contains", "Repository", "SourceRoot"));
        assert!(allowlist.allows("CustomRelation", "File", "Symbol"));
    }

    #[test]
    fn typed_tree_graph_rows_match_legacy_output_shape() {
        let root = SyntaxNode {
            node_type: "module".to_string(),
            text: "def foo():\n    pass\n".to_string(),
            line_start: Some(1),
            line_end: Some(2),
            byte_start: Some(0),
            byte_end: Some(20),
            capture_name: String::new(),
            children: vec![SyntaxNode {
                node_type: "function_definition".to_string(),
                text: "def foo():\n    pass".to_string(),
                line_start: Some(1),
                line_end: Some(2),
                byte_start: Some(0),
                byte_end: Some(19),
                capture_name: "definition.function".to_string(),
                children: Vec::new(),
                fields: BTreeMap::from([("name".to_string(), json!("foo"))]),
            }],
            fields: BTreeMap::new(),
        };
        let meta = BTreeMap::from([
            ("path".to_string(), "pkg/sample.py".to_string()),
            ("language".to_string(), "python".to_string()),
            ("source_root".to_string(), "/repo".to_string()),
            ("repository_label".to_string(), "repo".to_string()),
        ]);

        let typed = build_syntax_tree_graph_rows(meta.clone(), &root).unwrap();
        let legacy = build_tree_graph_output(&tree_graph_payload(meta, &root)).unwrap();
        let (legacy_node_types, legacy_edge_types) = output_types(&legacy);
        let mut typed_node_types = typed
            .nodes
            .iter()
            .map(|node| node.table.clone())
            .collect::<Vec<_>>();
        let mut typed_edge_types = typed
            .edges
            .iter()
            .map(|edge| edge.edge_type.clone())
            .collect::<Vec<_>>();
        typed_node_types.sort();
        typed_edge_types.sort();

        assert_eq!(typed.nodes.len(), legacy_node_types.len());
        assert_eq!(typed.edges.len(), legacy_edge_types.len());
        assert_eq!(typed_node_types, legacy_node_types);
        assert_eq!(typed_edge_types, legacy_edge_types);
    }

    fn tree_graph_payload(meta: BTreeMap<String, String>, root: &SyntaxNode) -> String {
        let mut lines = vec!["TREEGRAPH".to_string()];
        for (key, value) in meta {
            lines.push(format!("META\t{}\t{}", key, hex(&value)));
        }
        append_tree_graph_record(root, None, &mut 0, &mut lines);
        lines.join("\n") + "\n"
    }

    fn append_tree_graph_record(
        node: &SyntaxNode,
        parent_id: Option<usize>,
        next_id: &mut usize,
        lines: &mut Vec<String>,
    ) {
        let node_id = *next_id;
        *next_id += 1;
        let mut fields = vec![
            "NODE".to_string(),
            node_id.to_string(),
            parent_id.map(|value| value.to_string()).unwrap_or_default(),
            hex(&node.node_type),
            hex(&node.text),
            optional_i64(node.line_start),
            optional_i64(node.line_end),
            optional_i64(node.byte_start),
            optional_i64(node.byte_end),
            hex(&node.capture_name),
            node.fields.len().to_string(),
        ];
        for (key, value) in &node.fields {
            fields.push(hex(key));
            fields.push(hex(&serde_json::to_string(value).unwrap()));
        }
        lines.push(fields.join("\t"));
        for child in &node.children {
            append_tree_graph_record(child, Some(node_id), next_id, lines);
        }
    }

    fn output_types(output: &str) -> (Vec<String>, Vec<String>) {
        let mut node_types = Vec::new();
        let mut edge_types = Vec::new();
        for line in output.lines() {
            let parts = line.split('\t').collect::<Vec<_>>();
            match parts.first().copied() {
                Some("NODE") => node_types.push(unhex(parts[2]).unwrap()),
                Some("EDGE") => edge_types.push(unhex(parts[2]).unwrap()),
                _ => {}
            }
        }
        node_types.sort();
        edge_types.sort();
        (node_types, edge_types)
    }
}
