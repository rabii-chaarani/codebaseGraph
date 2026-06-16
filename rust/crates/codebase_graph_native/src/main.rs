use std::collections::{BTreeMap, HashMap};
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
}

struct Owner {
    node_id: String,
    table: String,
    qualified_name: String,
    scope_id: String,
}

fn main() -> Result<(), String> {
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
    let (meta, captures) = parse_input(&input)?;
    let mut builder = Builder::new(meta);
    builder.build(captures)?;
    print!("{}", builder.encode_output());
    Ok(())
}

fn first_record_kind(input: &str) -> Option<String> {
    input
        .lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| line.split('\t').next())
        .map(str::to_string)
}

impl Builder {
    fn new(meta: BTreeMap<String, String>) -> Self {
        Self {
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
        }
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

    fn emit_capture(&mut self, capture: &Capture, owner: &Owner) -> Result<(), String> {
        let syntax_id = self.syntax_capture(capture);
        let Some(table) = table_for_capture(&capture.capture_name, owner) else {
            return Ok(());
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
        self.edge(
            "Declares",
            &owner.node_id,
            &semantic.id,
            &format!("declares_{}", table.to_lowercase()),
            BTreeMap::new(),
        )?;
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
        if relation_allowed(edge_type, &source.table, &target.table) {
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

fn relation_allowed(edge_type: &str, source: &str, target: &str) -> bool {
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
}
