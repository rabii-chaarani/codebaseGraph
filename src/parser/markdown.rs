use super::captures::mapping_for_capture_name;
use super::ParseOutput;
use crate::normalize::SyntaxNode;
use crate::protocol::LanguageProfile;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) fn parse_markdown_source(source: &str, profile: &LanguageProfile) -> ParseOutput {
    let mut root = SyntaxNode {
        node_type: "Module".to_string(),
        text: String::new(),
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        capture_name: String::new(),
        children: Vec::new(),
        fields: BTreeMap::new(),
    };
    let total_lines = source.lines().count().max(1) as i64;
    push_markdown_capture(
        profile,
        "doc.source",
        markdown_node(
            "DocumentationSource",
            source,
            1,
            total_lines,
            0,
            source.len() as i64,
            [("name", ""), ("text", &summary(source))],
        ),
        &mut root,
    );
    for section in markdown_sections(source) {
        let heading = if section.heading.is_empty() {
            "section".to_string()
        } else {
            section.heading.clone()
        };
        push_markdown_capture(
            profile,
            "doc.chunk",
            markdown_node(
                "DocumentationChunk",
                &section.text,
                section.line_start,
                section.line_end,
                0,
                0,
                [
                    ("name", heading.as_str()),
                    ("heading", section.heading.as_str()),
                    ("text", summary(&section.text).as_str()),
                ],
            ),
            &mut root,
        );
    }
    ParseOutput {
        root,
        diagnostics: Vec::new(),
    }
}

fn push_markdown_capture(
    profile: &LanguageProfile,
    capture_name: &str,
    mut node: SyntaxNode,
    root: &mut SyntaxNode,
) {
    let Some(mapping) = mapping_for_capture_name(&profile.capture_mappings, capture_name) else {
        return;
    };
    node.capture_name = mapping.capture_name.clone();
    root.children.push(node);
}

fn markdown_node<const N: usize>(
    node_type: &str,
    text: &str,
    line_start: i64,
    line_end: i64,
    byte_start: i64,
    byte_end: i64,
    values: [(&str, &str); N],
) -> SyntaxNode {
    let mut fields = BTreeMap::new();
    for (key, value) in values {
        if !value.is_empty() {
            fields.insert(key.to_string(), json!(value));
        }
    }
    SyntaxNode {
        node_type: node_type.to_string(),
        text: text.to_string(),
        line_start: Some(line_start),
        line_end: Some(line_end),
        byte_start: if byte_end == 0 {
            None
        } else {
            Some(byte_start)
        },
        byte_end: if byte_end == 0 { None } else { Some(byte_end) },
        capture_name: String::new(),
        children: Vec::new(),
        fields,
    }
}

#[derive(Debug, Clone)]
struct MarkdownSection {
    heading: String,
    line_start: i64,
    line_end: i64,
    text: String,
}

fn markdown_sections(source: &str) -> Vec<MarkdownSection> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let level = trimmed
            .chars()
            .take_while(|character| *character == '#')
            .count();
        if (1..=6).contains(&level) && trimmed.chars().nth(level).is_some_and(char::is_whitespace) {
            headings.push((index + 1, trimmed[level..].trim().to_string()));
        }
    }
    if headings.is_empty() {
        let text = source.trim();
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![MarkdownSection {
                heading: String::new(),
                line_start: 1,
                line_end: lines.len().max(1) as i64,
                text: text.to_string(),
            }]
        };
    }
    headings
        .iter()
        .enumerate()
        .filter_map(|(index, (line_start, heading))| {
            let line_end = headings
                .get(index + 1)
                .map(|(next_line, _)| next_line.saturating_sub(1))
                .unwrap_or(lines.len());
            let text = lines[line_start.saturating_sub(1)..line_end]
                .join("\n")
                .trim()
                .to_string();
            if text.is_empty() {
                None
            } else {
                Some(MarkdownSection {
                    heading: heading.clone(),
                    line_start: *line_start as i64,
                    line_end: line_end as i64,
                    text,
                })
            }
        })
        .collect()
}

fn summary(text: &str) -> String {
    text.trim().chars().take(2000).collect()
}
