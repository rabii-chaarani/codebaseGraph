//! Strict event-based YAML decoding, retaining authored byte locations.
use super::policy::RunContext;
use super::{CompileError, SourceLocation};
use crate::architecture::RelativePath;
use libyaml_rs as yaml;
use std::{ffi::CStr, mem::MaybeUninit, slice};

#[derive(Debug)]
pub(super) struct Value {
    pub(super) location: SourceLocation,
    pub(super) quoted: bool,
    pub(super) kind: Kind,
}
#[derive(Debug)]
pub(super) enum Kind {
    Null,
    Bool(bool),
    Integer(serde_json::Number, bool),
    String(String),
    Sequence(Vec<Value>),
    Mapping(Vec<(Value, Value)>),
}

// Destroy nested values iteratively as well as constructing them iteratively. A
// configured depth is a policy bound, not a safe native call-stack limit.
impl Drop for Value {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        let mut kind = std::mem::replace(&mut self.kind, Kind::Null);
        loop {
            match kind {
                Kind::Sequence(children) => pending.extend(children),
                Kind::Mapping(entries) => {
                    for (key, value) in entries {
                        pending.push(key);
                        pending.push(value);
                    }
                }
                _ => {}
            }
            match pending.pop() {
                Some(mut child) => kind = std::mem::replace(&mut child.kind, Kind::Null),
                None => break,
            }
        }
    }
}

// The parser stores pointers into input, so both remain borrowed for its lifetime.
struct Parser<'a> {
    parser: Box<yaml::yaml_parser_t>,
    input: &'a str,
    path: &'a RelativePath,
    run: &'a RunContext,
    max_depth: u64,
}
struct Event(yaml::yaml_event_t);
impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: each Event wraps exactly one successfully initialized event.
        unsafe { yaml::yaml_event_delete(&mut self.0) }
    }
}
impl Drop for Parser<'_> {
    fn drop(&mut self) {
        // SAFETY: parser was successfully initialized and has not been deleted.
        unsafe { yaml::yaml_parser_delete(&mut *self.parser) }
    }
}
impl<'a> Parser<'a> {
    fn new(
        input: &'a str,
        path: &'a RelativePath,
        run: &'a RunContext,
        max_depth: u64,
    ) -> Result<Self, CompileError> {
        run.check()?;
        let mut parser = Box::<yaml::yaml_parser_t>::new_uninit();
        // SAFETY: parser points to allocated uninitialized storage. The input remains
        // alive and unchanged until after parser deletion, and its length is exact.
        unsafe {
            if !yaml::yaml_parser_initialize(parser.as_mut_ptr()).ok {
                return Err(CompileError::simple(
                    "resource_limit",
                    "YAML parser initialization failed",
                ));
            }
            let mut parser = parser.assume_init();
            yaml::yaml_parser_set_input_string(&mut *parser, input.as_ptr(), input.len() as u64);
            Ok(Self {
                parser,
                input,
                path,
                run,
                max_depth,
            })
        }
    }
    fn location(&self, start: yaml::yaml_mark_t, end: yaml::yaml_mark_t) -> SourceLocation {
        let start_byte = (start.index as usize).min(self.input.len());
        let end_byte = (end.index as usize).min(self.input.len());
        SourceLocation {
            path: self.path.clone(),
            start: start_byte,
            end: end_byte,
            line: start.line as usize + 1,
            column: start.column as usize + 1,
        }
    }
    fn error(&self, reason: impl Into<String>, mut location: SourceLocation) -> CompileError {
        if location.start == location.end && location.start < self.input.len() {
            if let Some(ch) = self.input[location.start..].chars().next() {
                location.end += ch.len_utf8();
            }
        }
        let reason = reason.into();
        let code = if reason == "duplicate mapping key" {
            "duplicate_key"
        } else if reason == "maximum parse depth exceeded" || reason == "parse depth overflow" {
            "resource_limit"
        } else {
            "architecture_invalid_input"
        };
        let mut error = CompileError::simple(code, reason);
        error.diagnostics[0].location = Some(location);
        error
    }
    fn next(&mut self) -> Result<Event, CompileError> {
        self.run.check()?;
        let mut event = MaybeUninit::<yaml::yaml_event_t>::uninit();
        // SAFETY: parser is live; on success the event is initialized and owned by Event.
        unsafe {
            if !yaml::yaml_parser_parse(&mut *self.parser, event.as_mut_ptr()).ok {
                let mark = self.parser.problem_mark;
                let reason = if self.parser.problem.is_null() {
                    "invalid YAML".into()
                } else {
                    CStr::from_ptr(self.parser.problem)
                        .to_string_lossy()
                        .into_owned()
                };
                let mut location = self.location(mark, mark);
                if location.start < self.input.len() {
                    location.end = location.start
                        + self.input[location.start..]
                            .chars()
                            .next()
                            .unwrap()
                            .len_utf8();
                }
                return Err(self.error(reason, location));
            }
            Ok(Event(event.assume_init()))
        }
    }
    fn node(&mut self, first: Event) -> Result<Value, CompileError> {
        use yaml::yaml_event_type_t::*;
        struct Frame {
            location: SourceLocation,
            mapping: bool,
            values: Vec<Value>,
            entries: Vec<(Value, Value)>,
            key: Option<Value>,
        }
        let mut frames: Vec<Frame> = Vec::new();
        let mut event = first;
        loop {
            self.run.check()?;
            let location = self.location(event.0.start_mark, event.0.end_mark);
            let depth = frames
                .len()
                .checked_add(1)
                .ok_or_else(|| self.error("parse depth overflow", location.clone()))?;
            let produced = match event.0.type_ {
                YAML_SEQUENCE_START_EVENT | YAML_MAPPING_START_EVENT => {
                    if depth as u64 > self.max_depth {
                        return Err(self.error("maximum parse depth exceeded", location));
                    }
                    // SAFETY: union member is chosen according to the event type.
                    let (anchor, tag) = unsafe {
                        if event.0.type_ == YAML_SEQUENCE_START_EVENT {
                            let s = event.0.data.sequence_start;
                            (s.anchor, s.tag)
                        } else {
                            let m = event.0.data.mapping_start;
                            (m.anchor, m.tag)
                        }
                    };
                    if !anchor.is_null() || !tag.is_null() {
                        return Err(self.error("anchors and tags are unsupported", location));
                    }
                    frames
                        .try_reserve(1)
                        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
                    frames.push(Frame {
                        location,
                        mapping: event.0.type_ == YAML_MAPPING_START_EVENT,
                        values: Vec::new(),
                        entries: Vec::new(),
                        key: None,
                    });
                    None
                }
                YAML_SEQUENCE_END_EVENT | YAML_MAPPING_END_EVENT => {
                    let Some(mut frame) = frames.pop() else {
                        return Err(self.error("unexpected collection end", location));
                    };
                    if frame.mapping != (event.0.type_ == YAML_MAPPING_END_EVENT)
                        || frame.key.is_some()
                    {
                        return Err(self.error("invalid collection end", location));
                    }
                    frame.location.end = location.end;
                    Some(Value {
                        location: frame.location,
                        quoted: false,
                        kind: if frame.mapping {
                            Kind::Mapping(frame.entries)
                        } else {
                            Kind::Sequence(frame.values)
                        },
                    })
                }
                YAML_SCALAR_EVENT => {
                    if depth as u64 > self.max_depth {
                        return Err(self.error("maximum parse depth exceeded", location));
                    }
                    // SAFETY: the event type selects scalar; libyaml owns its bytes until event deletion.
                    let scalar = unsafe { event.0.data.scalar };
                    if !scalar.anchor.is_null() || !scalar.tag.is_null() {
                        return Err(self.error("anchors and tags are unsupported", location));
                    }
                    let bytes =
                        unsafe { slice::from_raw_parts(scalar.value, scalar.length as usize) };
                    let value = std::str::from_utf8(bytes)
                        .map_err(|_| self.error("invalid UTF-8 scalar", location.clone()))?;
                    let kind = if scalar.style == yaml::yaml_scalar_style_t::YAML_PLAIN_SCALAR_STYLE
                    {
                        self.plain(value, &location)?
                    } else {
                        Kind::String(copy_string(value)?)
                    };
                    let quoted = matches!(
                        scalar.style,
                        yaml::yaml_scalar_style_t::YAML_SINGLE_QUOTED_SCALAR_STYLE
                            | yaml::yaml_scalar_style_t::YAML_DOUBLE_QUOTED_SCALAR_STYLE
                    );
                    Some(Value {
                        location,
                        quoted,
                        kind,
                    })
                }
                YAML_ALIAS_EVENT => return Err(self.error("aliases are unsupported", location)),
                _ => return Err(self.error("expected a YAML value", location)),
            };
            if let Some(value) = produced {
                if let Some(frame) = frames.last_mut() {
                    if frame.mapping {
                        if let Some(key) = frame.key.take() {
                            frame.entries.try_reserve(1).map_err(|_| {
                                CompileError::simple("resource_limit", "allocation failed")
                            })?;
                            frame.entries.push((key, value));
                        } else {
                            let Kind::String(ref name) = value.kind else {
                                return Err(self.error(
                                    "mapping key must be a string",
                                    value.location.clone(),
                                ));
                            };
                            if name == "<<" {
                                return Err(self
                                    .error("merge keys are unsupported", value.location.clone()));
                            }
                            for (key, _) in &frame.entries {
                                self.run.check()?;
                                if matches!(&key.kind, Kind::String(s) if s == name) {
                                    return Err(
                                        self.error("duplicate mapping key", value.location.clone())
                                    );
                                }
                            }
                            frame.key = Some(value);
                        }
                    } else {
                        frame.values.try_reserve(1).map_err(|_| {
                            CompileError::simple("resource_limit", "allocation failed")
                        })?;
                        frame.values.push(value);
                    }
                } else {
                    return Ok(value);
                }
            }
            event = self.next()?;
        }
    }
    fn plain(&self, value: &str, location: &SourceLocation) -> Result<Kind, CompileError> {
        match value {
            "" | "~" => {
                return Err(self.error(
                    "implicit empty or null scalar is unsupported",
                    location.clone(),
                ))
            }
            "true" => return Ok(Kind::Bool(true)),
            "false" => return Ok(Kind::Bool(false)),
            "null" => return Ok(Kind::Null),
            _ => {}
        }
        let numeric = value.strip_prefix('-').unwrap_or(value);
        if !numeric.is_empty() && numeric.bytes().all(|b| b.is_ascii_digit()) {
            if numeric.len() > 1 && numeric.starts_with('0') {
                return Err(self.error("leading-zero integer is unsupported", location.clone()));
            }
            let number = if value.starts_with('-') {
                value.parse::<i64>().ok().map(serde_json::Number::from)
            } else {
                value.parse::<u64>().ok().map(serde_json::Number::from)
            };
            return number
                .map(|number| Kind::Integer(number, value.starts_with('-')))
                .ok_or_else(|| self.error("integer out of range", location.clone()));
        }
        let mut lower = copy_string(value)?;
        lower.make_ascii_lowercase();
        let unsigned = lower.strip_prefix(['+', '-']).unwrap_or(&lower);
        let alternate = [("0x", 16), ("0o", 8), ("0b", 2)]
            .iter()
            .any(|(prefix, radix)| {
                unsigned.strip_prefix(prefix).is_some_and(|digits| {
                    !digits.is_empty()
                        && digits.bytes().all(|b| match radix {
                            16 => b.is_ascii_hexdigit(),
                            8 => (b'0'..=b'7').contains(&b),
                            2 => matches!(b, b'0' | b'1'),
                            _ => false,
                        })
                })
            });
        let plus_integer = value.starts_with('+')
            && !unsigned.is_empty()
            && unsigned.bytes().all(|b| b.is_ascii_digit());
        if matches!(lower.as_str(), ".inf" | "+.inf" | "-.inf" | ".nan")
            || alternate
            || plus_integer
            || looks_float(unsigned)
            || looks_date(value)
        {
            return Err(self.error("unsupported numeric or timestamp scalar", location.clone()));
        }
        Ok(Kind::String(copy_string(value)?))
    }
}
fn copy_string(value: &str) -> Result<String, CompileError> {
    let mut result = String::new();
    result
        .try_reserve(value.len())
        .map_err(|_| CompileError::simple("resource_limit", "allocation failed"))?;
    result.push_str(value);
    Ok(result)
}
fn looks_float(value: &str) -> bool {
    let b = value.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    let mut dot = false;
    if i < b.len() && b[i] == b'.' {
        dot = true;
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    let mut exponent = false;
    if i < b.len() && matches!(b[i], b'e' | b'E') {
        exponent = true;
        i += 1;
        if i < b.len() && matches!(b[i], b'+' | b'-') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if start == i {
            return false;
        }
    }
    i == b.len() && (dot || exponent)
}
fn looks_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() < 10
        || !b[..4].iter().all(u8::is_ascii_digit)
        || b[4] != b'-'
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || b[7] != b'-'
        || !b[8..10].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    if b.len() == 10 {
        return true;
    }
    if b.len() < 19
        || !matches!(b[10], b'T' | b't' | b' ')
        || !b[11..13].iter().all(u8::is_ascii_digit)
        || b[13] != b':'
        || !b[14..16].iter().all(u8::is_ascii_digit)
        || b[16] != b':'
        || !b[17..19].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let mut i = 19;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if i == b.len() {
        return true;
    }
    if i + 1 == b.len() && matches!(b[i], b'Z' | b'z') {
        return true;
    }
    i + 6 == b.len()
        && matches!(b[i], b'+' | b'-')
        && b[i + 1..i + 3].iter().all(u8::is_ascii_digit)
        && b[i + 3] == b':'
        && b[i + 4..i + 6].iter().all(u8::is_ascii_digit)
}

pub(super) fn parse(
    input: &[u8],
    path: &RelativePath,
    max_bytes: u64,
    max_depth: u64,
    run: &RunContext,
) -> Result<Value, CompileError> {
    run.check()?;
    if input.len() as u64 > max_bytes {
        return Err(CompileError::simple(
            "resource_limit",
            "YAML byte limit exceeded",
        ));
    }
    let input = std::str::from_utf8(input).map_err(|error| {
        let start = error.valid_up_to();
        let prefix = std::str::from_utf8(&input[..start]).unwrap_or("");
        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        let mut result = CompileError::simple("architecture_invalid_input", "input is not UTF-8");
        result.diagnostics[0].location = Some(SourceLocation {
            path: path.clone(),
            start,
            end: (start + error.error_len().unwrap_or(input.len() - start)).min(input.len()),
            line,
            column,
        });
        result
    })?;
    let mut parser = Parser::new(input, path, run, max_depth)?;
    use yaml::yaml_event_type_t::*;
    let stream = parser.next()?;
    if stream.0.type_ != YAML_STREAM_START_EVENT {
        return Err(parser.error(
            "expected stream",
            parser.location(stream.0.start_mark, stream.0.end_mark),
        ));
    }
    let doc = parser.next()?;
    if doc.0.type_ != YAML_DOCUMENT_START_EVENT {
        return Err(parser.error(
            "empty document",
            parser.location(doc.0.start_mark, doc.0.end_mark),
        ));
    }
    // SAFETY: document_start is active for this event, and directive pointers
    // remain valid until the document-start event is dropped.
    let directives = unsafe { doc.0.data.document_start };
    if directives.tag_directives.start != directives.tag_directives.end {
        return Err(parser.error(
            "tag directives are unsupported",
            parser.location(doc.0.start_mark, doc.0.end_mark),
        ));
    }
    if !directives.version_directive.is_null() {
        let version = unsafe { &*directives.version_directive };
        if version.major != 1 || version.minor != 2 {
            return Err(parser.error(
                "unsupported YAML version directive",
                parser.location(doc.0.start_mark, doc.0.end_mark),
            ));
        }
    }
    let next = parser.next()?;
    let value = parser.node(next)?;
    let end = parser.next()?;
    if end.0.type_ != YAML_DOCUMENT_END_EVENT {
        return Err(parser.error(
            "expected document end",
            parser.location(end.0.start_mark, end.0.end_mark),
        ));
    }
    let end = parser.next()?;
    if end.0.type_ != YAML_STREAM_END_EVENT {
        return Err(parser.error(
            "additional document is unsupported",
            parser.location(end.0.start_mark, end.0.end_mark),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::architecture::compile::{ArchitectureExecutionPolicy, ExecutionContext};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    fn run(input: &str, depth: u64, cancelled: bool) -> Result<Value, CompileError> {
        run_bytes(input.as_bytes(), depth, cancelled)
    }
    fn run_bytes(input: &[u8], depth: u64, cancelled: bool) -> Result<Value, CompileError> {
        let fields = [
            "max_closure_files",
            "max_file_bytes",
            "max_yaml_bytes",
            "max_parse_depth",
            "max_authored_entities",
            "max_expanded_rule_pairs",
            "max_mapping_records",
            "max_citations_per_mapping",
            "max_rationale_bytes",
            "max_proposal_operations",
            "max_scenarios",
            "max_projected_vertices",
            "max_projected_edges",
            "max_traversal_steps",
            "max_evidence_bytes",
            "max_result_bytes",
            "max_evaluation_duration_ms",
        ];
        let policy = ArchitectureExecutionPolicy::from_json(serde_json::Value::Object(
            fields
                .into_iter()
                .map(|k| (k.into(), 100000.into()))
                .collect(),
        ))
        .unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        flag.store(cancelled, Ordering::Relaxed);
        let context = ExecutionContext::new(flag, None).start(&policy).unwrap();
        parse(
            input,
            &RelativePath::new("model.yaml").unwrap(),
            10000,
            depth,
            &context,
        )
    }
    #[test]
    fn decoded_keys_and_unicode_spans() {
        let result = run("é: yes\n\"é\": no\n", 4, false).unwrap_err();
        let location = result.diagnostics()[0].location().unwrap();
        assert_eq!(
            (
                location.start(),
                location.end(),
                location.line(),
                location.column()
            ),
            (8, 12, 2, 1)
        );
        assert!(result.diagnostics()[0].reason().contains("duplicate"));
    }
    #[test]
    fn scalar_profile_and_forbidden_syntax() {
        for bad in [
            "x: &a value",
            "x: !custom value",
            "x: *a",
            "x: {<<: a}",
            "x: 01",
            "x: +1",
            "x: 1.2",
            "x: 2024-01-01",
            "x: ~",
            "x:",
            "x: 1\n---\ny: 2",
            "true: value",
        ] {
            assert!(run(bad, 8, false).is_err(), "accepted {bad}");
        }
        let value = run("[yes, '01', true, false, null, 42, -2]", 4, false).unwrap();
        let Kind::Sequence(values) = &value.kind else {
            panic!()
        };
        assert!(matches!(&values[0].kind, Kind::String(s) if s == "yes"));
        assert!(matches!(&values[1].kind, Kind::String(s) if s == "01"));
        assert!(matches!(values[2].kind, Kind::Bool(true)));
        assert!(matches!(values[3].kind, Kind::Bool(false)));
        assert!(matches!(values[4].kind, Kind::Null));
        assert!(matches!(&values[5].kind, Kind::Integer(n, _) if n.as_u64() == Some(42)));
        assert!(matches!(&values[6].kind, Kind::Integer(n, _) if n.as_i64() == Some(-2)));
    }
    #[test]
    fn deeply_nested_values_do_not_use_native_recursion() {
        let input = format!("{}x{}", "[".repeat(2000), "]".repeat(2000));
        let value = run(&input, 2001, false).unwrap();
        drop(value);
        assert!(run(&input, 2000, false).is_err());
    }

    #[test]
    fn limits_and_cancellation() {
        assert_eq!(run("[[x]]", 2, false).unwrap_err().code(), "resource_limit");
        assert!(run("2026-11-07-release", 2, false).is_ok());
        for text in ["nan", "inf", "infinity", "0xword", "1_000", "++1"] {
            assert!(
                matches!(run(text, 2, false).unwrap().kind, Kind::String(_)),
                "{text}"
            );
        }
        for text in ["1.2", "1e2", "0xff", "0o77", "0b01", "+1", "01"] {
            assert!(run(text, 2, false).is_err(), "{text}");
        }
        assert!(run("%YAML 1.1\n---\nx", 2, false).is_err());
        assert!(run("%TAG !a! tag:example.com,2026:\n---\nx", 2, false).is_err());
        let invalid = run_bytes(b"x: \xc3", 2, false).unwrap_err();
        let span = invalid.diagnostics()[0].location().unwrap();
        assert_eq!((span.start(), span.end()), (3, 4));
        assert_eq!(run("x", 2, true).unwrap_err().code(), "cancelled");
        assert!(run("{broken", 4, false).is_err());
    }
}
