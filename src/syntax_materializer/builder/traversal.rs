use super::{NativeBuilder, Owner};
use crate::syntax_materializer::{
    empty_metadata, fortran_literal_capture, module_label, semantic_child_ids,
    should_derive_root_module, table_for_capture, table_for_node_type, tree_capture,
    NativeSyntaxArena, TreeNodeRef,
};

impl NativeBuilder {
    pub(in crate::syntax_materializer) fn build_tree(
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

    pub(in crate::syntax_materializer) fn traverse_tree_node(
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

    pub(in crate::syntax_materializer) fn emit_tree_node(
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
}
