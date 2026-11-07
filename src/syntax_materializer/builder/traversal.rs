use super::{NativeBuilder, Owner};
use crate::syntax_materializer::{
    empty_metadata, fortran_literal_capture, module_label, semantic_child_ids,
    should_derive_root_module, table_for_capture, table_for_node_type, NativeSyntaxArena,
    TreeNodeRef,
};

impl NativeBuilder<'_, '_> {
    pub(in crate::syntax_materializer) fn build_tree(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        root_id: usize,
    ) -> Result<(), String> {
        let Some(root) = nodes.get_node(root_id) else {
            return Err("tree graph root node is missing".to_string());
        };
        let (repository_label, source_root, path) = if let Some(guard) = &mut self.guard {
            let copied = (|| {
                Ok::<_, crate::extraction_control::ExtractionError>((
                    crate::extraction_control::copy_text(&self.repository_label, guard)?,
                    crate::extraction_control::copy_text(&self.source_root, guard)?,
                    crate::extraction_control::copy_text(&self.path, guard)?,
                ))
            })();
            match copied {
                Ok(values) => values,
                Err(error) => {
                    let message = error.message.to_string();
                    self.guard_failure = Some(error);
                    return Err(message);
                }
            }
        } else {
            (
                self.repository_label.clone(),
                self.source_root.clone(),
                self.path.clone(),
            )
        };
        let repository = self.support_node("Repository", &repository_label, &repository_label, "");
        let source = self.support_node("SourceRoot", &source_root, &source_root, &source_root);
        let file_label = if let Some(guard) = &mut self.guard {
            let mut start = 0usize;
            let result = (|| {
                for (index, ch) in path.char_indices() {
                    guard.visit(0)?;
                    if ch == '/' {
                        start = index + 1;
                    }
                }
                crate::extraction_control::copy_text(&path[start..], guard)
            })();
            match result {
                Ok(value) => value,
                Err(error) => {
                    let message = error.message.to_string();
                    self.guard_failure = Some(error);
                    return Err(message);
                }
            }
        } else {
            path.rsplit('/').next().unwrap_or(path.as_str()).to_string()
        };
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
        self.materialize_syntax_tree(nodes, root_id, &file.id)?;

        if matches!(
            root.node_type(),
            "Module" | "module" | "program" | "source_file"
        ) {
            let root_capture = self.capture(root)?;
            let syntax_id = self.syntax_capture(&root_capture);
            let module_name = if let Some(guard) = &mut self.guard {
                match crate::syntax_materializer::guarded_module_label(&path, guard) {
                    Ok(value) => value,
                    Err(error) => {
                        let message = error.message.to_string();
                        self.guard_failure = Some(error);
                        return Err(message);
                    }
                }
            } else {
                module_label(&path)
            };
            let module =
                self.semantic_node("Module", &root_capture, &module_name, &file.id, "", None);
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
                node_id: self.copy_text(&module.id)?,
                table: "Module".to_string(),
                qualified_name: self.copy_text(&module.qualified_name)?,
                scope_id: self.copy_text(&module_scope.id)?,
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
                node_id: self.copy_text(&file.id)?,
                table: "File".to_string(),
                qualified_name: self.copy_text(&file.qualified_name)?,
                scope_id: self.copy_text(&file_scope.id)?,
            };
            self.traverse_tree_node(nodes, root_id, &owner)?;
        }
        Ok(())
    }

    fn materialize_syntax_tree(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        root_id: usize,
        file_id: &str,
    ) -> Result<(), String> {
        let Some(root) = nodes.get_node(root_id) else {
            return Err("tree graph root node is missing".to_string());
        };
        let root_capture = self.capture(root)?;
        let root_syntax_id = self.syntax_capture(&root_capture);
        self.edge(
            "EvidencedBy",
            file_id,
            &root_syntax_id,
            "syntax_root",
            empty_metadata(),
        )?;
        self.materialize_syntax_children(nodes, root, &root_syntax_id)
    }

    fn materialize_syntax_children(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        parent: TreeNodeRef<'_>,
        parent_syntax_id: &str,
    ) -> Result<(), String> {
        let mut stack = Vec::new();
        self.reserve(&mut stack, parent.children.len())?;
        for (index, child_id) in parent.children.iter().enumerate().rev() {
            stack.push((*child_id, index, self.copy_text(parent_syntax_id)?));
        }
        while let Some((child_id, child_index, parent_id)) = stack.pop() {
            self.guard_visit()?;
            let child = nodes
                .get_node(child_id)
                .ok_or_else(|| format!("tree graph node {child_id} is missing"))?;
            let child_capture = self.capture(child)?;
            let child_syntax_id = self.syntax_capture(&child_capture);
            self.syntax_child(
                &parent_id,
                &child_syntax_id,
                child.field_name(),
                child_index,
            )?;
            self.reserve(&mut stack, child.children.len())?;
            for (index, next_id) in child.children.iter().enumerate().rev() {
                stack.push((*next_id, index, self.copy_text(&child_syntax_id)?));
            }
        }
        Ok(())
    }

    pub(in crate::syntax_materializer) fn traverse_tree_node(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        node_id: usize,
        owner: &Owner,
    ) -> Result<(), String> {
        enum Job {
            Enter(usize, usize),
            Exit(usize, usize),
        }
        let mut owners = Vec::new();
        self.reserve(&mut owners, 1)?;
        owners.push(Owner {
            node_id: self.copy_text(&owner.node_id)?,
            table: self.copy_text(&owner.table)?,
            qualified_name: self.copy_text(&owner.qualified_name)?,
            scope_id: self.copy_text(&owner.scope_id)?,
        });
        let mut stack = Vec::new();
        self.reserve(&mut stack, 1)?;
        stack.push(Job::Enter(node_id, 0));
        while let Some(job) = stack.pop() {
            self.guard_visit()?;
            match job {
                Job::Enter(id, owner_index) => {
                    let node = nodes
                        .get_node(id)
                        .ok_or_else(|| format!("tree graph node {id} is missing"))?;
                    let next = self.emit_tree_node(nodes, node, &owners[owner_index])?;
                    let child_owner = if let Some(value) = next {
                        self.reserve(&mut owners, 1)?;
                        owners.push(value);
                        owners.len() - 1
                    } else {
                        owner_index
                    };
                    let children = if let Some(guard) = &mut self.guard {
                        match crate::syntax_materializer::semantic_child_ids_guarded(
                            nodes,
                            node,
                            &self.language,
                            guard,
                        ) {
                            Ok(children) => children,
                            Err(error) => {
                                let message = error.message.to_string();
                                self.guard_failure = Some(error);
                                return Err(message);
                            }
                        }
                    } else {
                        semantic_child_ids(nodes, node, &self.language)
                    };
                    self.reserve(&mut stack, children.len().saturating_add(1))?;
                    stack.push(Job::Exit(id, child_owner));
                    for child in children.into_iter().rev() {
                        stack.push(Job::Enter(child, child_owner));
                    }
                }
                Job::Exit(id, owner_index) => {
                    let node = nodes
                        .get_node(id)
                        .ok_or_else(|| format!("tree graph node {id} is missing"))?;
                    self.emit_parser_like_metadata_fields(node, &owners[owner_index])?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::syntax_materializer) fn emit_tree_node(
        &mut self,
        nodes: &NativeSyntaxArena<'_>,
        node: TreeNodeRef<'_>,
        owner: &Owner,
    ) -> Result<Option<Owner>, String> {
        let mut capture = self.capture(node)?;
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
                node_id: self.copy_text(&semantic.id)?,
                table,
                qualified_name: self.copy_text(&semantic.qualified_name)?,
                scope_id: self.copy_text(&scope.id)?,
            }));
        }
        Ok(None)
    }
}
