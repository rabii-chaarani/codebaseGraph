use super::*;

pub(super) struct NativeSyntaxArena<'a> {
    pub(super) nodes: Vec<NativeSyntaxNode<'a>>,
    pub(super) root_id: usize,
}

pub(super) struct NativeSyntaxNode<'a> {
    pub(super) parent_id: Option<usize>,
    pub(super) node: &'a SyntaxNode,
    pub(super) children: Vec<usize>,
}

impl<'a> NativeSyntaxArena<'a> {
    pub(super) fn new(root: &'a SyntaxNode) -> Self {
        let mut arena = Self {
            nodes: Vec::new(),
            root_id: 0,
        };
        arena.root_id = arena.append(root, None);
        arena
    }

    pub(super) fn append(&mut self, node: &'a SyntaxNode, parent_id: Option<usize>) -> usize {
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

    pub(super) fn get_node(&self, id: usize) -> Option<TreeNodeRef<'_>> {
        self.nodes.get(id).map(|node| TreeNodeRef {
            parent_id: node.parent_id,
            children: &node.children,
            node: node.node,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct TreeNodeRef<'a> {
    pub(super) parent_id: Option<usize>,
    pub(super) children: &'a [usize],
    pub(super) node: &'a SyntaxNode,
}

impl TreeNodeRef<'_> {
    pub(super) fn node_type(&self) -> &str {
        &self.node.node_type
    }

    pub(super) fn text(&self) -> &str {
        &self.node.text
    }

    pub(super) fn line_start(&self) -> Option<i64> {
        self.node.line_start
    }

    pub(super) fn line_end(&self) -> Option<i64> {
        self.node.line_end
    }

    pub(super) fn byte_start(&self) -> Option<i64> {
        self.node.byte_start
    }

    pub(super) fn byte_end(&self) -> Option<i64> {
        self.node.byte_end
    }

    pub(super) fn capture_name(&self) -> &str {
        &self.node.capture_name
    }

    pub(super) fn field_keys(&self) -> Vec<String> {
        self.node.fields.keys().cloned().collect()
    }

    pub(super) fn field_value(&self, field: &str) -> Option<Value> {
        self.node.fields.get(field).cloned()
    }

    pub(super) fn field_label(&self, field: &str) -> Option<String> {
        self.field_value(field).as_ref().and_then(json_value_label)
    }
}
