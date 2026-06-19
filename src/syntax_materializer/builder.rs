use super::*;

pub(super) struct Owner {
    pub(super) node_id: String,
    pub(super) table: String,
    pub(super) qualified_name: String,
    pub(super) scope_id: String,
}

pub(super) struct NativeBuilder {
    pub(super) path: String,
    pub(super) language: String,
    pub(super) source_root: String,
    pub(super) repository_label: String,
    pub(super) nodes: HashMap<String, GraphNodeRow>,
    pub(super) edges: HashMap<String, GraphEdgeRow>,
    pub(super) symbols_by_name: HashMap<String, Vec<String>>,
    pub(super) relation_allowlist: RelationAllowlist,
}

impl NativeBuilder {
    pub(super) fn new(meta: BTreeMap<String, String>) -> Result<Self, String> {
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

    pub(super) fn build_tree(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        root_id: usize,
    ) -> Result<(), String> {
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

    pub(super) fn traverse_tree_node(
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

    pub(super) fn emit_tree_node(
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

    pub(super) fn emit_tree_import(
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

    pub(super) fn emit_tree_assignment(
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

    pub(super) fn emit_tree_parameters(
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

    pub(super) fn emit_tree_return_type(
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

    pub(super) fn emit_type_annotation_capture(
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

    pub(super) fn emit_type_annotation_label(
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

    pub(super) fn emit_parser_like_metadata_fields(
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

    pub(super) fn emit_import(
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

    pub(super) fn emit_declaration(
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

    pub(super) fn emit_call(
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

    pub(super) fn emit_reference(
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

    pub(super) fn emit_simple_semantic(
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

    pub(super) fn emit_reference_edges(
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

    pub(super) fn resolve_reference_target(&mut self, label: &str) -> Option<GraphNodeRow> {
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

    pub(super) fn support_node(
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

    pub(super) fn semantic_node(
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

    pub(super) fn symbol_node(&mut self, label: &str) -> GraphNodeRow {
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

    pub(super) fn scope_for(&mut self, owner: &GraphNodeRow) -> GraphNodeRow {
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

    pub(super) fn syntax_capture(&mut self, capture: &Capture) -> String {
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

    pub(super) fn connect_owner(
        &mut self,
        owner: &Owner,
        semantic: &GraphNodeRow,
    ) -> Result<(), String> {
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

    pub(super) fn derived_from(
        &mut self,
        semantic_id: &str,
        syntax_id: &str,
    ) -> Result<(), String> {
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

    pub(super) fn edge_if_allowed(
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

    pub(super) fn edge(
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

    pub(super) fn add_node(&mut self, node: GraphNodeRow) -> GraphNodeRow {
        self.nodes
            .entry(node.id.clone())
            .or_insert_with(|| node.clone());
        let added = self.nodes.get(&node.id).cloned().unwrap_or(node);
        self.register_resolvable(&added);
        added
    }

    pub(super) fn register_resolvable(&mut self, node: &GraphNodeRow) {
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

    pub(super) fn into_rows(self) -> BuiltGraphRows {
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
