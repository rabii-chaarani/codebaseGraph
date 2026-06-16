use crate::error::NativeError;
use crate::normalize::{mapping_for_target, NativeCapture};
use crate::protocol::{LanguageProfile, SourceSnapshot};
use std::fs;

pub(crate) fn parse_file(
    snapshot: &SourceSnapshot,
    profile: &LanguageProfile,
) -> Result<Vec<NativeCapture>, NativeError> {
    let source = fs::read_to_string(&snapshot.absolute_path)?;
    Ok(parse_source(&source, profile))
}

fn parse_source(source: &str, profile: &LanguageProfile) -> Vec<NativeCapture> {
    let mut captures = Vec::new();
    let mut byte_cursor = 0usize;
    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            byte_cursor += line.len() + 1;
            continue;
        }
        match profile.language.as_str() {
            "python" => parse_python_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            "rust" => parse_rust_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            "go" => parse_go_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            "c" | "cpp" => parse_c_like_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            "fortran" => parse_fortran_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            "markdown" => parse_markdown_line(
                profile,
                trimmed,
                line,
                line_number,
                byte_cursor,
                &mut captures,
            ),
            _ => {}
        }
        byte_cursor += line.len() + 1;
    }
    captures
}

#[allow(clippy::too_many_arguments)]
fn push_capture(
    profile: &LanguageProfile,
    target: &str,
    capture_prefix: &str,
    fallback_node_type: &str,
    label: String,
    text: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if label.is_empty() {
        return;
    }
    let Some(mapping) = mapping_for_target(&profile.capture_mappings, target, capture_prefix)
    else {
        return;
    };
    let node_type = mapping
        .parser_node_types
        .first()
        .map(String::as_str)
        .unwrap_or(fallback_node_type);
    captures.push(NativeCapture::from_mapping(
        mapping,
        node_type,
        label,
        text.to_string(),
        line_number,
        byte_cursor,
    ));
}

fn parse_python_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if let Some(name) = trimmed
        .strip_prefix("class ")
        .and_then(identifier_before_suffix)
    {
        push_capture(
            profile,
            "Class",
            "definition.class",
            "class_definition",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = trimmed
        .strip_prefix("def ")
        .and_then(identifier_before_suffix)
    {
        push_capture(
            profile,
            "Function",
            "definition.function",
            "function_definition",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(module) = trimmed.strip_prefix("import ") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.import",
            "import_statement",
            module_label(module),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(module) = trimmed.strip_prefix("from ") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.import",
            "import_from_statement",
            module_label(module),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
    for call in call_labels(trimmed) {
        push_capture(
            profile,
            "CallExpression",
            "reference.call",
            "call",
            call,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn parse_rust_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if let Some(name) = keyword_name(trimmed, "struct ") {
        push_capture(
            profile,
            "Class",
            "definition.struct",
            "struct_item",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = keyword_name(trimmed, "enum ") {
        push_capture(
            profile,
            "Class",
            "definition.enum",
            "enum_item",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = keyword_name(trimmed, "trait ") {
        push_capture(
            profile,
            "Class",
            "definition.interface",
            "trait_item",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = keyword_name(trimmed.trim_start_matches("pub "), "fn ") {
        push_capture(
            profile,
            "Function",
            "definition.function",
            "function_item",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(rest) = trimmed.strip_prefix("use ") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.use",
            "use_declaration",
            rest.trim_end_matches(';').to_string(),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
    for call in call_labels(trimmed) {
        push_capture(
            profile,
            "CallExpression",
            "reference.call",
            "call_expression",
            call,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn parse_go_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if let Some(name) = trimmed
        .strip_prefix("package ")
        .map(|value| value.trim().to_string())
    {
        push_capture(
            profile,
            "Module",
            "definition.package",
            "package_clause",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = keyword_name(trimmed, "func ") {
        push_capture(
            profile,
            "Function",
            "definition.function",
            "function_declaration",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if trimmed.starts_with("import ") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.import",
            "import_declaration",
            module_label(trimmed.trim_start_matches("import ")),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
    for call in call_labels(trimmed) {
        push_capture(
            profile,
            "CallExpression",
            "reference.call",
            "call_expression",
            call,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn parse_c_like_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if let Some(include) = trimmed.strip_prefix("#include") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.include",
            "preproc_include",
            module_label(include),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = keyword_name(trimmed, "struct ") {
        push_capture(
            profile,
            "Class",
            "definition.struct",
            "struct_specifier",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = c_function_name(trimmed) {
        push_capture(
            profile,
            "Function",
            "definition.function",
            "function_definition",
            name,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
    for call in call_labels(trimmed) {
        push_capture(
            profile,
            "CallExpression",
            "reference.call",
            "call_expression",
            call,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn parse_fortran_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    let lower = trimmed.to_lowercase();
    if let Some(name) = lower
        .strip_prefix("module ")
        .map(|_| trimmed[7..].trim().to_string())
    {
        if !name.to_lowercase().starts_with("procedure") {
            push_capture(
                profile,
                "Module",
                "definition.module",
                "module",
                name,
                line,
                line_number,
                byte_cursor,
                captures,
            );
        }
    } else if let Some(name) = lower
        .strip_prefix("subroutine ")
        .map(|_| trimmed[11..].trim().to_string())
    {
        push_capture(
            profile,
            "Function",
            "definition.subroutine",
            "subroutine",
            identifier_prefix(&name),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if let Some(name) = lower
        .strip_prefix("function ")
        .map(|_| trimmed[9..].trim().to_string())
    {
        push_capture(
            profile,
            "Function",
            "definition.function",
            "function",
            identifier_prefix(&name),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    } else if lower.starts_with("use ") {
        push_capture(
            profile,
            "ImportDeclaration",
            "reference.use",
            "use_statement",
            trimmed[4..].trim().to_string(),
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn parse_markdown_line(
    profile: &LanguageProfile,
    trimmed: &str,
    line: &str,
    line_number: usize,
    byte_cursor: usize,
    captures: &mut Vec<NativeCapture>,
) {
    if trimmed.starts_with('#') {
        let label = trimmed.trim_start_matches('#').trim().to_string();
        push_capture(
            profile,
            "DocumentationChunk",
            "definition.section",
            "atx_heading",
            label,
            line,
            line_number,
            byte_cursor,
            captures,
        );
    }
}

fn identifier_before_suffix(value: &str) -> Option<String> {
    let value = value.trim();
    let end = value.find(['(', ':', '{']).unwrap_or(value.len());
    Some(identifier_prefix(&value[..end]))
}

fn keyword_name(line: &str, keyword: &str) -> Option<String> {
    line.strip_prefix(keyword)
        .map(identifier_prefix)
        .filter(|name| !name.is_empty())
}

fn identifier_prefix(value: &str) -> String {
    value
        .trim()
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | ':' | '.'))
        .collect()
}

fn module_label(value: &str) -> String {
    value
        .trim()
        .trim_matches(';')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('<')
        .trim_matches('>')
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

fn c_function_name(line: &str) -> Option<String> {
    if !line.contains('(')
        || line.starts_with("if ")
        || line.starts_with("for ")
        || line.starts_with("while ")
    {
        return None;
    }
    let before_paren = line.split('(').next()?.trim();
    before_paren
        .split_whitespace()
        .last()
        .map(identifier_prefix)
        .filter(|name| !name.is_empty())
}

fn call_labels(line: &str) -> Vec<String> {
    let mut labels = Vec::new();
    for part in line.split('(').take(8) {
        let Some(candidate) = part
            .split(|character: char| {
                !character.is_alphanumeric() && character != '_' && character != '.'
            })
            .next_back()
        else {
            continue;
        };
        let candidate = candidate.trim_matches('.');
        if candidate.is_empty()
            || matches!(
                candidate,
                "if" | "for" | "while" | "switch" | "return" | "def" | "fn" | "func"
            )
        {
            continue;
        }
        labels.push(candidate.to_string());
    }
    labels.sort();
    labels.dedup();
    labels
}
