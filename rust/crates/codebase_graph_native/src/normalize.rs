use crate::protocol::CaptureMapping;

#[derive(Debug, Clone)]
pub(crate) struct NativeCapture {
    pub(crate) capture_name: String,
    pub(crate) node_type: String,
    pub(crate) label: String,
    pub(crate) text: String,
    pub(crate) line_start: Option<i64>,
    pub(crate) line_end: Option<i64>,
    pub(crate) byte_start: Option<i64>,
    pub(crate) byte_end: Option<i64>,
    pub(crate) fields: Vec<String>,
}

impl NativeCapture {
    pub(crate) fn from_mapping(
        mapping: &CaptureMapping,
        node_type: &str,
        label: String,
        text: String,
        line_number: usize,
        byte_start: usize,
    ) -> Self {
        Self {
            capture_name: mapping.capture_name.clone(),
            node_type: node_type.to_string(),
            label,
            text,
            line_start: Some(line_number as i64),
            line_end: Some(line_number as i64),
            byte_start: Some(byte_start as i64),
            byte_end: None,
            fields: Vec::new(),
        }
    }
}

pub(crate) fn mapping_for_target<'a>(
    mappings: &'a [CaptureMapping],
    target: &str,
    preferred_capture_prefix: &str,
) -> Option<&'a CaptureMapping> {
    mappings
        .iter()
        .find(|mapping| {
            mapping.target_node_type == target
                && mapping.capture_name.starts_with(preferred_capture_prefix)
        })
        .or_else(|| {
            mappings
                .iter()
                .find(|mapping| mapping.target_node_type == target)
        })
}
