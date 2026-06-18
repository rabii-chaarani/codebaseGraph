use crate::graph_rows::{BuiltGraphRows, GraphEdgeRow, GraphNodeRow};
use crate::normalize::SyntaxNode;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) fn build_syntax_tree_graph_rows(
    meta: BTreeMap<String, String>,
    root: &SyntaxNode,
) -> Result<BuiltGraphRows, String> {
    let mut builder = NativeBuilder::new(meta)?;
    let nodes = NativeSyntaxArena::new(root);
    builder.build_tree(&nodes, nodes.root_id)?;
    Ok(builder.into_rows())
}

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

#[derive(Deserialize)]
struct RelationSpecPayload {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source_types: Vec<String>,
    #[serde(default)]
    target_types: Vec<String>,
}

#[derive(Clone, Default)]
struct RelationAllowlist {
    enabled: bool,
    pairs_by_relation: HashMap<String, HashSet<(String, String)>>,
}

impl RelationAllowlist {
    fn from_meta(meta: &BTreeMap<String, String>) -> Result<Self, String> {
        let Some(encoded) = meta.get("ontology_relations") else {
            return Ok(Self::default());
        };
        let relation_specs: Vec<RelationSpecPayload> = serde_json::from_str(encoded)
            .map_err(|error| format!("invalid ontology_relations metadata: {error}"))?;
        let mut pairs_by_relation: HashMap<String, HashSet<(String, String)>> = HashMap::new();
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

struct NativeBuilder {
    path: String,
    language: String,
    source_root: String,
    repository_label: String,
    nodes: HashMap<String, GraphNodeRow>,
    edges: HashMap<String, GraphEdgeRow>,
    symbols_by_name: HashMap<String, Vec<String>>,
    relation_allowlist: RelationAllowlist,
}

impl NativeBuilder {
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
            nodes: HashMap::new(),
            edges: HashMap::new(),
            symbols_by_name: HashMap::new(),
            relation_allowlist,
        })
    }

    fn build_tree(&mut self, nodes: &NativeSyntaxArena<'_>, root_id: usize) -> Result<(), String> {
        let Some(root) = nodes.get_node(root_id) else {
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
            empty_metadata(),
        )?;
        self.edge(
            "Contains",
            &source.id,
            &file.id,
            "source_root_file",
            empty_metadata(),
        )?;

        if matches!(
            root.node_type(),
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
                empty_metadata(),
            )?;
            self.edge(
                "Contains",
                &module.id,
                &module_scope.id,
                "module_contains_scope",
                empty_metadata(),
            )?;
            self.edge(
                "HasScope",
                &module.id,
                &module_scope.id,
                "module_scope",
                empty_metadata(),
            )?;
            if should_derive_root_module(&self.language, root.node_type()) {
                self.derived_from(&module.id, &syntax_id)?;
            }
            let owner = Owner {
                node_id: module.id.clone(),
                table: "Module".to_string(),
                qualified_name: module.qualified_name.clone(),
                scope_id: module_scope.id.clone(),
            };
            for child_id in root.children {
                self.traverse_tree_node(nodes, *child_id, &owner)?;
            }
        } else {
            let file_scope = self.scope_for(&file);
            self.edge(
                "HasScope",
                &file.id,
                &file_scope.id,
                "file_scope",
                empty_metadata(),
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
        nodes: &NativeSyntaxArena<'_>,
        node_id: usize,
        owner: &Owner,
    ) -> Result<(), String> {
        let Some(node) = nodes.get_node(node_id) else {
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
        nodes: &NativeSyntaxArena<'_>,
        node: TreeNodeRef<'_>,
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
                empty_metadata(),
            )?;
            self.edge(
                "HasScope",
                &semantic.id,
                &scope.id,
                &format!("{}_scope", table.to_lowercase()),
                empty_metadata(),
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

    fn emit_tree_import(
        &mut self,
        node: TreeNodeRef<'_>,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<GraphNodeRow, String> {
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
        nodes: &NativeSyntaxArena<'_>,
        node: TreeNodeRef<'_>,
        owner: &Owner,
        syntax_id: &str,
    ) -> Result<GraphNodeRow, String> {
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
                empty_metadata(),
            )?;
            self.edge_if_allowed(
                "Assigns",
                &assignment.id,
                &target.id,
                "assignment_target",
                empty_metadata(),
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
                    empty_metadata(),
                )?;
            }
        }

        if let Some(value_id) = call_value_child(nodes, node) {
            let Some(value_node) = nodes.get_node(value_id) else {
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
                empty_metadata(),
            )?;
        }
        Ok(assignment)
    }

    fn emit_tree_parameters(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        function_node: TreeNodeRef<'_>,
        callable: &GraphNodeRow,
    ) -> Result<(), String> {
        for (index, parameter_id) in parameter_child_ids(nodes, function_node)
            .into_iter()
            .enumerate()
        {
            let Some(parameter_node) = nodes.get_node(parameter_id) else {
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
                empty_metadata(),
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
                    empty_metadata(),
                )?;
            }
        }
        Ok(())
    }

    fn emit_tree_return_type(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        function_node: TreeNodeRef<'_>,
        callable: &GraphNodeRow,
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
            empty_metadata(),
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
            empty_metadata(),
        )?;
        self.derived_from(&return_node.id, &syntax_id)?;
        Ok(())
    }

    fn emit_type_annotation_capture(
        &mut self,
        capture: &Capture,
        owner_id: &str,
        owner_qualified_name: &str,
    ) -> Result<GraphNodeRow, String> {
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
    ) -> Result<GraphNodeRow, String> {
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
        node: TreeNodeRef<'_>,
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
    ) -> Result<GraphNodeRow, String> {
        let imported = capture.label.clone();
        let mut metadata = empty_metadata();
        metadata.insert("imported_name".to_string(), Value::String(imported.clone()));
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
            empty_metadata(),
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
                empty_metadata(),
            )?;
            self.edge(
                "EvidencedBy",
                &dependency.id,
                syntax_id,
                "parser_evidence",
                empty_metadata(),
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
    ) -> Result<GraphNodeRow, String> {
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
            empty_metadata(),
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
                empty_metadata(),
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
    ) -> Result<GraphNodeRow, String> {
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
                empty_metadata(),
            )?;
        }
        if let Some(target) = self.emit_reference_edges(&call, &call.label, "call")? {
            self.edge_if_allowed(
                "Calls",
                &call.id,
                &target.id,
                "call_target",
                empty_metadata(),
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
    ) -> Result<GraphNodeRow, String> {
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
    ) -> Result<GraphNodeRow, String> {
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
                empty_metadata(),
            )?;
        }
        if table == "ReturnType" {
            self.edge_if_allowed(
                "HasReturnType",
                &owner.node_id,
                &semantic.id,
                "captured_return_type",
                empty_metadata(),
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
                empty_metadata(),
            )?;
        }
        if table == "TypeAnnotation" {
            self.edge_if_allowed(
                "HasTypeAnnotation",
                &owner.node_id,
                &semantic.id,
                "captured_type_annotation",
                empty_metadata(),
            )?;
            self.emit_reference_edges(&semantic, &semantic.label, "type_annotation")?;
        }
        if matches!(table, "DocumentationSource" | "DocumentationChunk") {
            self.edge_if_allowed(
                "Documents",
                &semantic.id,
                &owner.node_id,
                "documents_owner",
                empty_metadata(),
            )?;
            self.edge_if_allowed(
                "EvidencedBy",
                &semantic.id,
                syntax_id,
                "parser_evidence",
                empty_metadata(),
            )?;
        }
        self.derived_from(&semantic.id, syntax_id)?;
        Ok(semantic)
    }

    fn emit_reference_edges(
        &mut self,
        source: &GraphNodeRow,
        label: &str,
        kind_prefix: &str,
    ) -> Result<Option<GraphNodeRow>, String> {
        let Some(target) = self.resolve_reference_target(label) else {
            return Ok(None);
        };
        if target.id == source.id {
            return Ok(None);
        }
        let mut metadata = empty_metadata();
        metadata.insert("label".to_string(), Value::String(label.to_string()));
        metadata.insert("resolver".to_string(), Value::String("label".to_string()));
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

    fn resolve_reference_target(&mut self, label: &str) -> Option<GraphNodeRow> {
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

    fn support_node(
        &mut self,
        table: &str,
        stable_key: &str,
        label: &str,
        path: &str,
    ) -> GraphNodeRow {
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.to_string()),
        );
        let node = GraphNodeRow {
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
            metadata: Value::Object(metadata),
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
        metadata: Option<Map<String, Value>>,
    ) -> GraphNodeRow {
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
        let mut node_metadata = empty_metadata();
        node_metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
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
        let node = GraphNodeRow {
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
            metadata: Value::Object(node_metadata),
        };
        self.add_node(node)
    }

    fn symbol_node(&mut self, label: &str) -> GraphNodeRow {
        let stable_key = format!("{}|Symbol|{}", self.path, label.trim());
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
        );
        metadata.insert(
            "resolution".to_string(),
            Value::String("name_placeholder".to_string()),
        );
        let node = GraphNodeRow {
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
            metadata: Value::Object(metadata),
        };
        self.add_node(node)
    }

    fn scope_for(&mut self, owner: &GraphNodeRow) -> GraphNodeRow {
        let stable_key = format!("{}|{}|scope", self.path, owner.id);
        let mut metadata = empty_metadata();
        metadata.insert(
            "canonical_key".to_string(),
            Value::String(stable_key.clone()),
        );
        let node = GraphNodeRow {
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
            metadata: Value::Object(metadata),
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
        let mut metadata = empty_metadata();
        metadata.insert("canonical_key".to_string(), Value::String(stable_key));
        metadata.insert(
            "fields".to_string(),
            Value::Array(
                capture
                    .fields
                    .iter()
                    .map(|field| Value::String(field.clone()))
                    .collect(),
            ),
        );
        let summary = capture.text.chars().take(160).collect();
        let node = GraphNodeRow {
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
            metadata: Value::Object(metadata),
        };
        self.add_node(node);
        syntax_id
    }

    fn connect_owner(&mut self, owner: &Owner, semantic: &GraphNodeRow) -> Result<(), String> {
        self.edge(
            "Contains",
            &owner.node_id,
            &semantic.id,
            &format!("contains_{}", semantic.table.to_lowercase()),
            empty_metadata(),
        )?;
        if !owner.scope_id.is_empty() {
            self.edge(
                "Contains",
                &owner.scope_id,
                &semantic.id,
                &format!("scope_contains_{}", semantic.table.to_lowercase()),
                empty_metadata(),
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
                empty_metadata(),
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
        metadata: Map<String, Value>,
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
        metadata: Map<String, Value>,
    ) -> Result<GraphEdgeRow, String> {
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
        let mut edge_metadata = empty_metadata();
        edge_metadata.insert(
            "canonical_key".to_string(),
            Value::String(canonical_key.clone()),
        );
        for (key, value) in metadata {
            edge_metadata.insert(key, value);
        }
        let edge = GraphEdgeRow {
            id: graph_id("edge", &canonical_key),
            edge_type: edge_type.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            kind: kind.to_string(),
            confidence: 1.0,
            line_start: None,
            line_end: None,
            byte_start: None,
            byte_end: None,
            metadata: Value::Object(edge_metadata),
        };
        self.edges
            .entry(edge.id.clone())
            .or_insert_with(|| edge.clone());
        Ok(self.edges.get(&edge.id).cloned().unwrap_or(edge))
    }

    fn add_node(&mut self, node: GraphNodeRow) -> GraphNodeRow {
        self.nodes
            .entry(node.id.clone())
            .or_insert_with(|| node.clone());
        let added = self.nodes.get(&node.id).cloned().unwrap_or(node);
        self.register_resolvable(&added);
        added
    }

    fn register_resolvable(&mut self, node: &GraphNodeRow) {
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

    fn into_rows(self) -> BuiltGraphRows {
        let mut nodes = self.nodes.into_values().collect::<Vec<_>>();
        nodes.sort_by(|left, right| {
            (left.table.as_str(), left.id.as_str()).cmp(&(right.table.as_str(), right.id.as_str()))
        });
        let mut edges = self.edges.into_values().collect::<Vec<_>>();
        edges.sort_by(|left, right| {
            (left.edge_type.as_str(), left.id.as_str())
                .cmp(&(right.edge_type.as_str(), right.id.as_str()))
        });
        BuiltGraphRows { nodes, edges }
    }
}

struct NativeSyntaxArena<'a> {
    nodes: Vec<NativeSyntaxNode<'a>>,
    root_id: usize,
}

struct NativeSyntaxNode<'a> {
    parent_id: Option<usize>,
    node: &'a SyntaxNode,
    children: Vec<usize>,
}

impl<'a> NativeSyntaxArena<'a> {
    fn new(root: &'a SyntaxNode) -> Self {
        let mut arena = Self {
            nodes: Vec::new(),
            root_id: 0,
        };
        arena.root_id = arena.append(root, None);
        arena
    }

    fn append(&mut self, node: &'a SyntaxNode, parent_id: Option<usize>) -> usize {
        let node_id = self.nodes.len();
        self.nodes.push(NativeSyntaxNode {
            parent_id,
            node,
            children: Vec::new(),
        });
        let children = node
            .children
            .iter()
            .map(|child| self.append(child, Some(node_id)))
            .collect();
        self.nodes[node_id].children = children;
        node_id
    }

    fn get_node(&self, id: usize) -> Option<TreeNodeRef<'_>> {
        self.nodes.get(id).map(|node| TreeNodeRef {
            parent_id: node.parent_id,
            children: &node.children,
            node: node.node,
        })
    }
}

#[derive(Clone, Copy)]
struct TreeNodeRef<'a> {
    parent_id: Option<usize>,
    children: &'a [usize],
    node: &'a SyntaxNode,
}

impl TreeNodeRef<'_> {
    fn node_type(&self) -> &str {
        &self.node.node_type
    }

    fn text(&self) -> &str {
        &self.node.text
    }

    fn line_start(&self) -> Option<i64> {
        self.node.line_start
    }

    fn line_end(&self) -> Option<i64> {
        self.node.line_end
    }

    fn byte_start(&self) -> Option<i64> {
        self.node.byte_start
    }

    fn byte_end(&self) -> Option<i64> {
        self.node.byte_end
    }

    fn capture_name(&self) -> &str {
        &self.node.capture_name
    }

    fn field_keys(&self) -> Vec<String> {
        self.node.fields.keys().cloned().collect()
    }

    fn field_value(&self, field: &str) -> Option<Value> {
        self.node.fields.get(field).cloned()
    }

    fn field_label(&self, field: &str) -> Option<String> {
        self.field_value(field).as_ref().and_then(json_value_label)
    }
}

fn tree_capture(node: TreeNodeRef<'_>) -> Capture {
    Capture {
        capture_name: node.capture_name().to_string(),
        node_type: node.node_type().to_string(),
        label: tree_label(node),
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: node.field_keys(),
    }
}

fn tree_label(node: TreeNodeRef<'_>) -> String {
    for key in ["name", "id", "arg", "attr", "module", "path", "function"] {
        if let Some(label) = node.field_label(key) {
            if !label.is_empty() {
                return label;
            }
        }
    }
    if let Some(label) = node.field_label("value") {
        if !label.is_empty() {
            return label;
        }
    }
    let text = node.text().trim();
    if text.is_empty() {
        node.node_type().to_string()
    } else {
        text.to_string()
    }
}

fn should_derive_root_module(language: &str, root_node_type: &str) -> bool {
    !(matches!(root_node_type, "source_file")
        || (language == "python" && root_node_type == "module")
        || (language == "markdown" && root_node_type == "Module"))
}

fn json_value_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Object(object) => {
            for key in ["id", "name", "arg", "attr", "value"] {
                if let Some(value) = object.get(key) {
                    let label = json_value_label(value)?;
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
    nodes: &NativeSyntaxArena<'_>,
    parent: TreeNodeRef<'_>,
    language: &str,
) -> Vec<usize> {
    let mut child_ids = Vec::new();
    for child_id in parent.children {
        let Some(child) = nodes.get_node(*child_id) else {
            continue;
        };
        if should_inline_child(child, language) {
            child_ids.extend(semantic_child_ids(nodes, child, language));
        } else if should_traverse_child(parent, child, language) {
            child_ids.push(*child_id);
        }
    }
    child_ids
}

fn should_inline_child(child: TreeNodeRef<'_>, language: &str) -> bool {
    (language == "python" && child.node_type() == "block")
        || (language == "fortran" && child.node_type() == "variable_declaration")
}

fn should_traverse_child(parent: TreeNodeRef<'_>, child: TreeNodeRef<'_>, language: &str) -> bool {
    if language == "python" {
        if parent.node_type() == "attribute" {
            return json_field_label(parent, "value")
                .is_some_and(|label| label == tree_label(child));
        }
        if matches!(child.node_type(), "parameters" | "decorator") {
            return false;
        }
        if matches!(
            parent.node_type(),
            "class_definition" | "function_definition"
        ) && matches!(child.node_type(), "identifier" | "type" | "type_identifier")
        {
            return false;
        }
        if matches!(child.node_type(), "identifier" | "type_identifier")
            && !matches!(parent.node_type(), "assignment" | "call" | "attribute")
        {
            return false;
        }
    }
    if child.node_type() == "block" {
        return true;
    }
    if matches!(
        parent.node_type(),
        "import_statement" | "import_from_statement" | "import_declaration" | "use_declaration"
    ) && matches!(
        child.node_type(),
        "identifier"
            | "dotted_name"
            | "aliased_import"
            | "import_list"
            | "string"
            | "interpreted_string_literal"
            | "raw_string_literal"
            | "string_literal"
    ) {
        if language == "python" && child.node_type() == "dotted_name" {
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

fn json_field_label(node: TreeNodeRef<'_>, field: &str) -> Option<String> {
    node.field_label(field)
}

fn import_label(node: TreeNodeRef<'_>) -> Option<String> {
    let module = json_field_label(node, "module").unwrap_or_default();
    let names = node
        .field_value("names")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| json_value_label(&item))
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

fn assignment_target_label(nodes: &NativeSyntaxArena<'_>, node: TreeNodeRef<'_>) -> Option<String> {
    json_field_label(node, "target").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get_node(*child_id))
            .find(|child| !matches!(child.node_type(), "call" | "call_expression"))
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

fn call_value_child(nodes: &NativeSyntaxArena<'_>, node: TreeNodeRef<'_>) -> Option<usize> {
    node.children.iter().copied().find(|child_id| {
        nodes
            .get_node(*child_id)
            .is_some_and(|child| matches!(child.node_type(), "call" | "call_expression"))
    })
}

fn parameter_child_ids(
    nodes: &NativeSyntaxArena<'_>,
    function_node: TreeNodeRef<'_>,
) -> Vec<usize> {
    function_node
        .children
        .iter()
        .filter_map(|child_id| nodes.get_node(*child_id))
        .filter(|child| child.node_type() == "parameters")
        .flat_map(|parameters| parameters.children.iter().copied())
        .filter(|child_id| {
            nodes.get_node(*child_id).is_some_and(|child| {
                matches!(
                    child.node_type(),
                    "identifier" | "typed_parameter" | "default_parameter" | "parameter"
                )
            })
        })
        .collect()
}

fn parameter_label(node: TreeNodeRef<'_>) -> String {
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

fn parameter_capture(node: TreeNodeRef<'_>, language: &str) -> Capture {
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
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: vec!["arg".to_string()],
    }
}

fn parameter_annotation_label(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<String> {
    json_field_label(node, "annotation").or_else(|| {
        node.children
            .iter()
            .filter_map(|child_id| nodes.get_node(*child_id))
            .find(|child| {
                matches!(
                    child.node_type(),
                    "type" | "type_identifier" | "qualified_type" | "annotation"
                )
            })
            .map(tree_label)
            .filter(|label| !label.is_empty())
    })
}

fn parameter_annotation_capture(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
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
            text: node.text().to_string(),
            line_start: node.line_start(),
            line_end: node.line_end(),
            byte_start: node.byte_start(),
            byte_end: node.byte_end(),
            fields: vec!["id".to_string()],
        });
    }
    parameter_annotation_label(nodes, node).map(|label| Capture {
        capture_name: String::new(),
        node_type: "type_annotation".to_string(),
        label,
        text: node.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: Vec::new(),
    })
}

fn return_type_capture(
    nodes: &NativeSyntaxArena<'_>,
    function_node: TreeNodeRef<'_>,
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
            text: function_node.text().to_string(),
            line_start: function_node.line_start(),
            line_end: function_node.line_end(),
            byte_start: function_node.byte_start(),
            byte_end: function_node.byte_end(),
            fields: Vec::new(),
        })
}

fn first_child_with_type<'a>(
    nodes: &'a NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
    node_types: &[&str],
) -> Option<TreeNodeRef<'a>> {
    node.children.iter().find_map(|child_id| {
        let child = nodes.get_node(*child_id)?;
        if node_types
            .iter()
            .any(|node_type| child.node_type() == *node_type)
        {
            Some(child)
        } else {
            None
        }
    })
}

fn fortran_literal_capture(
    nodes: &NativeSyntaxArena<'_>,
    node: TreeNodeRef<'_>,
) -> Option<Capture> {
    if node.node_type() != "intrinsic_type" {
        return None;
    }
    let parent = node
        .parent_id
        .and_then(|parent_id| nodes.get_node(parent_id))?;
    if parent.node_type() != "variable_declaration" {
        return None;
    }
    Some(Capture {
        capture_name: String::new(),
        node_type: "integer".to_string(),
        label: parent.text().to_string(),
        text: parent.text().to_string(),
        line_start: node.line_start(),
        line_end: node.line_end(),
        byte_start: node.byte_start(),
        byte_end: node.byte_end(),
        fields: Vec::new(),
    })
}

fn parser_like_metadata_capture(node: TreeNodeRef<'_>, field_name: &str) -> Option<Capture> {
    let metadata = node.field_value(field_name)?;
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

fn metadata_object_label(object: &Map<String, Value>) -> Option<String> {
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

fn metadata_value_label(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Array(items) => Some(format!(
            "[{}]",
            items
                .iter()
                .filter_map(metadata_value_label)
                .map(|item| format!("'{item}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        Value::Object(object) => metadata_object_label(object),
        Value::Null => None,
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

fn imported_name(node: &GraphNodeRow) -> String {
    node.metadata
        .get("imported_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn symbol_key(label: &str) -> String {
    label.trim().to_lowercase()
}

fn graph_id(prefix: &str, value: &str) -> String {
    format!("{}:{}", prefix, sha1_hex(value.as_bytes()))
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

fn stable_optional_i64(value: Option<i64>) -> String {
    value
        .map(|item| item.to_string())
        .unwrap_or_else(|| "None".to_string())
}

fn empty_metadata() -> Map<String, Value> {
    Map::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_cli;
    use serde_json::json;

    #[test]
    fn syntax_materializer_rows_match_legacy_for_python_tree() {
        let root = syntax_node(
            "module",
            "class Service:\n    def handle(self):\n        return call()\n",
            vec![syntax_node(
                "class_definition",
                "class Service:\n    def handle(self):\n        return call()",
                vec![syntax_node(
                    "function_definition",
                    "def handle(self):\n        return call()",
                    Vec::new(),
                    &[("name", json!("handle"))],
                )],
                &[("name", json!("Service"))],
            )],
            &[],
        );

        assert_native_matches_legacy(meta("python", "pkg/service.py"), &root);
    }

    #[test]
    fn syntax_materializer_rows_match_legacy_for_rust_tree() {
        let root = syntax_node(
            "source_file",
            "fn handle() { call(); }",
            vec![syntax_node(
                "function_item",
                "fn handle() { call(); }",
                Vec::new(),
                &[("name", json!("handle"))],
            )],
            &[],
        );

        assert_native_matches_legacy(meta("rust", "src/lib.rs"), &root);
    }

    #[test]
    fn syntax_materializer_rows_match_legacy_for_go_tree() {
        let root = syntax_node(
            "source_file",
            "package main\nfunc Handle() { Call() }\n",
            vec![syntax_node(
                "function_declaration",
                "func Handle() { Call() }",
                Vec::new(),
                &[("name", json!("Handle"))],
            )],
            &[],
        );

        assert_native_matches_legacy(meta("go", "main.go"), &root);
    }

    #[test]
    fn syntax_materializer_rows_match_legacy_for_empty_module_tree() {
        let root = syntax_node("module", "", Vec::new(), &[]);

        assert_native_matches_legacy(meta("python", "empty.py"), &root);
    }

    #[test]
    fn syntax_materializer_duplicate_node_and_edge_behavior_matches_legacy() {
        let duplicate = syntax_node(
            "function_definition",
            "def same():\n    pass",
            Vec::new(),
            &[("name", json!("same"))],
        );
        let root = syntax_node(
            "module",
            "def same():\n    pass\ndef same():\n    pass\n",
            vec![duplicate.clone(), duplicate],
            &[],
        );

        let native = build_syntax_tree_graph_rows(meta("python", "pkg/dupe.py"), &root).unwrap();
        let legacy =
            legacy_cli::build_syntax_tree_graph_rows(meta("python", "pkg/dupe.py"), &root).unwrap();

        assert_eq!(native.nodes, legacy.nodes);
        assert_eq!(native.edges, legacy.edges);
        assert_eq!(
            native
                .nodes
                .iter()
                .filter(|node| node.table == "Function" && node.label == "same")
                .count(),
            1
        );
    }

    #[test]
    fn syntax_materializer_manifest_ids_remain_stable() {
        let root = syntax_node(
            "module",
            "import os\nVALUE = call()\n",
            vec![
                syntax_node(
                    "import_statement",
                    "import os",
                    Vec::new(),
                    &[("module", json!("os"))],
                ),
                syntax_node(
                    "assignment",
                    "VALUE = call()",
                    vec![syntax_node("call", "call()", Vec::new(), &[])],
                    &[("target", json!("VALUE"))],
                ),
            ],
            &[],
        );

        let rows = build_syntax_tree_graph_rows(meta("python", "pkg/stable.py"), &root).unwrap();
        let ids = rows
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"Module:e1d78e658a62137527fd"));
        assert!(ids.contains(&"ImportDeclaration:0b00e4257b4bd2af9e92"));
        assert!(ids.contains(&"Constant:f30f45c3854762d38187"));
    }

    fn assert_native_matches_legacy(meta: BTreeMap<String, String>, root: &SyntaxNode) {
        let native = build_syntax_tree_graph_rows(meta.clone(), root).unwrap();
        let legacy = legacy_cli::build_syntax_tree_graph_rows(meta, root).unwrap();

        assert_eq!(native, legacy);
    }

    fn meta(language: &str, path: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("path".to_string(), path.to_string()),
            ("language".to_string(), language.to_string()),
            ("source_root".to_string(), "/repo".to_string()),
            ("repository_label".to_string(), "repo".to_string()),
        ])
    }

    fn syntax_node(
        node_type: &str,
        text: &str,
        children: Vec<SyntaxNode>,
        fields: &[(&str, Value)],
    ) -> SyntaxNode {
        SyntaxNode {
            node_type: node_type.to_string(),
            text: text.to_string(),
            line_start: Some(1),
            line_end: Some(text.lines().count().max(1) as i64),
            byte_start: Some(0),
            byte_end: Some(text.len() as i64),
            capture_name: String::new(),
            children,
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }
}
