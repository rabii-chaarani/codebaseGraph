use super::*;

pub(super) fn is_symbol_target(table: &str) -> bool {
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

pub(super) fn is_declaration(table: &str) -> bool {
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

pub(super) fn is_expression(table: &str) -> bool {
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

pub(super) fn is_documentation(table: &str) -> bool {
    matches!(table, "DocumentationSource" | "DocumentationChunk")
}

pub(super) fn module_label(path: &str) -> String {
    let stem = path.rsplit_once('.').map(|(left, _)| left).unwrap_or(path);
    stem.replace('/', ".")
}

pub(super) fn qualified_name(owner: &str, label: &str) -> String {
    if owner.is_empty() || owner == label {
        label.to_string()
    } else if label.is_empty() {
        owner.to_string()
    } else {
        format!("{}.{}", owner, label)
    }
}

pub(super) fn kind_for(table: &str, node_type: &str) -> String {
    match table {
        "Method" => "method".to_string(),
        "Function" => "function".to_string(),
        "Class" => "class".to_string(),
        _ => node_type.to_string(),
    }
}

pub(super) fn imported_name(node: &GraphNodeRow) -> String {
    node.metadata
        .get("imported_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(super) fn symbol_key(label: &str) -> String {
    label.trim().to_lowercase()
}

pub(super) fn graph_id(prefix: &str, value: &str) -> String {
    format!("{}:{}", prefix, sha1_hex(value.as_bytes()))
}

pub(super) fn sha1_hex(bytes: &[u8]) -> String {
    let digest = sha1(bytes);
    digest[..10]
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

pub(super) fn sha1(input: &[u8]) -> [u8; 20] {
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

pub(super) fn stable_optional_i64(value: Option<i64>) -> String {
    value
        .map(|item| item.to_string())
        .unwrap_or_else(|| "None".to_string())
}

pub(super) fn empty_metadata() -> Map<String, Value> {
    Map::new()
}
