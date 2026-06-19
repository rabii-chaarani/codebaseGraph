use super::connectors::{
    ConnectorRow, ConnectorRowKey, ConnectorRowsByTypePair, ConnectorTypePair,
};
use std::collections::HashMap;

pub(super) fn sorted_keys<V>(values: &HashMap<String, V>) -> Vec<&String> {
    let mut keys = values.keys().collect::<Vec<_>>();
    keys.sort();
    keys
}

pub(super) fn sorted_row_values<V>(rows: &HashMap<String, V>) -> Vec<&V> {
    let mut entries = rows.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

pub(super) fn sorted_connector_type_buckets(
    buckets: &ConnectorRowsByTypePair,
) -> Vec<(&ConnectorTypePair, &HashMap<ConnectorRowKey, ConnectorRow>)> {
    let mut buckets = buckets.iter().collect::<Vec<_>>();
    buckets.sort_by(|left, right| left.0.cmp(right.0));
    buckets
}

pub(super) fn sorted_connector_rows(
    rows: &HashMap<ConnectorRowKey, ConnectorRow>,
) -> Vec<&ConnectorRow> {
    let mut entries = rows.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries.into_iter().map(|(_, value)| value).collect()
}
