use super::{NativeBuilder, Owner};
use crate::graph_rows::GraphNodeRow;
use crate::syntax_materializer::{
    assignment_target_label, assignment_target_table, call_value_child, empty_metadata,
    import_label, json_field_label, parameter_annotation_capture, parameter_capture,
    parameter_child_ids, parser_like_metadata_capture, return_type_capture, symbol_key,
    table_for_node_type, tree_capture, Capture, NativeSyntaxArena, TreeNodeRef,
};
use serde_json::Value;

impl NativeBuilder {
    pub(in crate::syntax_materializer) fn emit_tree_import(
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

    pub(in crate::syntax_materializer) fn emit_tree_assignment(
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

    pub(in crate::syntax_materializer) fn emit_tree_parameters(
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

    pub(in crate::syntax_materializer) fn emit_tree_return_type(
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

    pub(in crate::syntax_materializer) fn emit_type_annotation_capture(
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

    pub(in crate::syntax_materializer) fn emit_type_annotation_label(
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

    pub(in crate::syntax_materializer) fn emit_parser_like_metadata_fields(
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

    pub(in crate::syntax_materializer) fn emit_import(
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

    pub(in crate::syntax_materializer) fn emit_declaration(
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

    pub(in crate::syntax_materializer) fn emit_call(
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

    pub(in crate::syntax_materializer) fn emit_reference(
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

    pub(in crate::syntax_materializer) fn emit_simple_semantic(
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

    pub(in crate::syntax_materializer) fn emit_reference_edges(
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

    pub(in crate::syntax_materializer) fn resolve_reference_target(
        &mut self,
        label: &str,
    ) -> Option<GraphNodeRow> {
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
}
