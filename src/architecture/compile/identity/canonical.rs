use super::super::{policy::RunContext, CompileError};
use serde_json::Value;

fn artifact_key(v: &Value) -> (&str, &str, &str, &str) {
    let get = |k| v.get(k).and_then(Value::as_str).unwrap_or("");
    (
        get("repository"),
        get("commit"),
        get("path"),
        get("content_digest"),
    )
}

// This receives only fixed-depth typed records. Scenario recursion is emitted by
// output's explicit job stack instead of descending through serde.
pub(in crate::architecture::compile) fn normalize(
    v: &mut Value,
    run: &RunContext,
) -> Result<(), CompileError> {
    run.check()?;
    match v {
        Value::Object(map) => {
            for (_, value) in map.iter_mut() {
                normalize(value, run)?;
            }
            let is_edge = map.contains_key("projection")
                && map.contains_key("from")
                && map.contains_key("to")
                && map.get("from").is_some_and(Value::is_array);
            for (key, value) in map.iter_mut() {
                let Some(array) = value.as_array_mut() else {
                    continue;
                };
                match key.as_str() {
                    "responsibilities" => {
                        sort_by(array, run, |a, b| a["id"].as_str().cmp(&b["id"].as_str()))?
                    }
                    "responsibility_ids" | "origins" => {
                        sort_by(array, run, |a, b| a.as_str().cmp(&b.as_str()))?
                    }
                    "from" | "to" if is_edge => {
                        sort_by(array, run, |a, b| a.as_str().cmp(&b.as_str()))?
                    }
                    "contract_refs" | "expected" => {
                        sort_by(array, run, |a, b| artifact_key(a).cmp(&artifact_key(b)))?
                    }
                    _ => {}
                }
                run.check()?;
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize(value, run)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// In-place heapsort with a cooperative check on every sift step.
pub(in crate::architecture::compile) fn sort_by<T>(
    values: &mut [T],
    run: &RunContext,
    mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering,
) -> Result<(), CompileError> {
    fn sift<T>(
        values: &mut [T],
        mut root: usize,
        end: usize,
        run: &RunContext,
        cmp: &mut impl FnMut(&T, &T) -> std::cmp::Ordering,
    ) -> Result<(), CompileError> {
        loop {
            run.check()?;
            let Some(left) = root.checked_mul(2).and_then(|n| n.checked_add(1)) else {
                return Ok(());
            };
            if left >= end {
                return Ok(());
            }
            let right = left + 1;
            let child = if right < end && cmp(&values[left], &values[right]).is_lt() {
                right
            } else {
                left
            };
            if !cmp(&values[root], &values[child]).is_lt() {
                return Ok(());
            }
            values.swap(root, child);
            root = child;
        }
    }
    let len = values.len();
    for start in (0..len / 2).rev() {
        sift(values, start, len, run, &mut cmp)?;
    }
    for end in (1..len).rev() {
        run.check()?;
        values.swap(0, end);
        sift(values, 0, end, run, &mut cmp)?;
    }
    run.check()
}
