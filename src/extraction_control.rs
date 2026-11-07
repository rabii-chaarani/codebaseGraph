//! Neutral cooperative resource accounting for in-memory source extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExtractionError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}
impl ExtractionError {
    pub(crate) fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
}
pub(crate) struct ExtractionControl<'a> {
    checkpoint: &'a mut dyn FnMut() -> Result<(), ExtractionError>,
    max_depth: u64,
    max_steps: u64,
    max_vertices: u64,
    max_edges: u64,
    steps: u64,
    vertices: u64,
    edges: u64,
    #[cfg(test)]
    reserve_failure_after: Option<usize>,
    #[cfg(test)]
    edge_copy_observer: Option<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
    #[cfg(test)]
    assignment_copy_observer: Option<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
    #[cfg(test)]
    assignment_classification_observer:
        Option<(String, std::sync::Arc<std::sync::atomic::AtomicBool>)>,
}
impl<'a> ExtractionControl<'a> {
    pub(crate) fn new(
        checkpoint: &'a mut dyn FnMut() -> Result<(), ExtractionError>,
        max_depth: u64,
        max_steps: u64,
        max_vertices: u64,
        max_edges: u64,
    ) -> Self {
        Self {
            checkpoint,
            max_depth,
            max_steps,
            max_vertices,
            max_edges,
            steps: 0,
            vertices: 0,
            edges: 0,
            #[cfg(test)]
            reserve_failure_after: None,
            #[cfg(test)]
            edge_copy_observer: None,
            #[cfg(test)]
            assignment_copy_observer: None,
            #[cfg(test)]
            assignment_classification_observer: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn inject_reserve_failure_after(&mut self, successful_reserves: usize) {
        self.reserve_failure_after = Some(successful_reserves);
    }
    #[cfg(test)]
    pub(crate) fn watch_edge_copy(
        &mut self,
        label: String,
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.edge_copy_observer = Some((label, flag));
    }
    #[cfg(test)]
    pub(crate) fn observe_edge_copy(&self, label: Option<&str>) {
        if let Some((expected, flag)) = &self.edge_copy_observer {
            if label == Some(expected.as_str()) {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn watch_assignment_copy(
        &mut self,
        label: String,
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.assignment_copy_observer = Some((label, flag));
    }
    #[cfg(test)]
    pub(crate) fn observe_assignment_copy(&self, label: &str) {
        if let Some((expected, flag)) = &self.assignment_copy_observer {
            if expected == label {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn watch_assignment_classification(
        &mut self,
        label: String,
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.assignment_classification_observer = Some((label, flag));
    }
    #[cfg(test)]
    pub(crate) fn observe_assignment_classification(&self, label: &str) {
        if let Some((expected, flag)) = &self.assignment_classification_observer {
            if expected == label {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    pub(crate) fn reserve<T>(
        &mut self,
        values: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), ExtractionError> {
        self.check()?;
        #[cfg(test)]
        if let Some(remaining) = self.reserve_failure_after.as_mut() {
            if *remaining == 0 {
                return Err(ExtractionError::new("resource_limit", "allocation failed"));
            }
            *remaining -= 1;
        }
        values
            .try_reserve(additional)
            .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))
    }
    pub(crate) fn check(&mut self) -> Result<(), ExtractionError> {
        (self.checkpoint)()
    }
    pub(crate) fn visit(&mut self, depth: u64) -> Result<(), ExtractionError> {
        self.check()?;
        if depth > self.max_depth {
            return Err(ExtractionError::new(
                "resource_limit",
                "source parse depth exceeded",
            ));
        }
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or_else(|| ExtractionError::new("resource_limit", "source traversal overflow"))?;
        if self.steps > self.max_steps {
            return Err(ExtractionError::new(
                "resource_limit",
                "source traversal exceeded",
            ));
        }
        Ok(())
    }
    pub(crate) fn vertex(&mut self) -> Result<(), ExtractionError> {
        self.check()?;
        self.vertices = self
            .vertices
            .checked_add(1)
            .ok_or_else(|| ExtractionError::new("resource_limit", "source vertex overflow"))?;
        if self.vertices > self.max_vertices {
            return Err(ExtractionError::new(
                "resource_limit",
                "source vertices exceeded",
            ));
        }
        Ok(())
    }
    pub(crate) fn edge(&mut self) -> Result<(), ExtractionError> {
        self.check()?;
        self.edges = self
            .edges
            .checked_add(1)
            .ok_or_else(|| ExtractionError::new("resource_limit", "source edge overflow"))?;
        if self.edges > self.max_edges {
            return Err(ExtractionError::new(
                "resource_limit",
                "source edges exceeded",
            ));
        }
        Ok(())
    }
}

/// Cooperative in-place heapsort, with no sorting allocation.
/// Each sift is actual traversal work and consumes a step. Input order from a
/// hash table may vary, so near-boundary admission can vary with comparison
/// count; no successful result omits output when the budget is exhausted.
pub(crate) fn sort_by<T>(
    values: &mut [T],
    control: &mut ExtractionControl<'_>,
    mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering,
) -> Result<(), ExtractionError> {
    fn sift<T>(
        values: &mut [T],
        mut root: usize,
        end: usize,
        control: &mut ExtractionControl<'_>,
        cmp: &mut impl FnMut(&T, &T) -> std::cmp::Ordering,
    ) -> Result<(), ExtractionError> {
        loop {
            control.visit(0)?;
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
        sift(values, start, len, control, &mut cmp)?;
    }
    for end in (1..len).rev() {
        control.visit(0)?;
        values.swap(0, end);
        sift(values, 0, end, control, &mut cmp)?;
    }
    Ok(())
}

pub(crate) fn copy_text(
    value: &str,
    control: &mut ExtractionControl<'_>,
) -> Result<String, ExtractionError> {
    let mut output = String::new();
    output
        .try_reserve(value.len())
        .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
    for ch in value.chars() {
        control.visit(0)?;
        output.push(ch);
    }
    Ok(output)
}

/// Match `str::trim().to_lowercase()` with fallible, cooperative growth.
pub(crate) fn normalized_symbol(
    value: &str,
    control: &mut ExtractionControl<'_>,
) -> Result<String, ExtractionError> {
    let mut first = None;
    let mut last = 0usize;
    let mut ascii = true;
    for (index, ch) in value.char_indices() {
        control.visit(0)?;
        ascii &= ch.is_ascii();
        if !ch.is_whitespace() {
            if first.is_none() {
                first = Some(index);
            }
            last = index + ch.len_utf8();
        }
    }
    let Some(first) = first else {
        return Ok(String::new());
    };
    let trimmed = &value[first..last];
    if !ascii {
        // Unicode lowercase has context-sensitive mappings (for example final
        // sigma); preserve std semantics until its allocation can be bounded.
        let output = trimmed.to_lowercase();
        control.check()?;
        return Ok(output);
    }
    let mut output = String::new();
    for ch in trimmed.chars() {
        control.visit(0)?;
        for lower in ch.to_lowercase() {
            output
                .try_reserve(lower.len_utf8())
                .map_err(|_| ExtractionError::new("resource_limit", "allocation failed"))?;
            output.push(lower);
        }
    }
    Ok(output)
}

/// Borrow the same slice as `str::trim` while polling each scanned character.
pub(crate) fn trim_text<'a>(
    value: &'a str,
    control: &mut ExtractionControl<'_>,
) -> Result<&'a str, ExtractionError> {
    let mut first = None;
    let mut end = 0;
    for (index, ch) in value.char_indices() {
        control.visit(0)?;
        if !ch.is_whitespace() {
            if first.is_none() {
                first = Some(index);
            }
            end = index + ch.len_utf8();
        }
    }
    Ok(first.map_or("", |start| &value[start..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symbol_normalization_matches_unicode_context() {
        for value in [" ABC ", "ΟΣ", "Straße", "İ"] {
            let mut check = || Ok(());
            let mut control = ExtractionControl::new(&mut check, 100, 1000, 1, 1);
            assert_eq!(
                normalized_symbol(value, &mut control).unwrap(),
                value.trim().to_lowercase()
            );
        }
    }
    #[test]
    fn exact_depth_and_step_boundaries() {
        let mut check = || Ok(());
        let mut exact = ExtractionControl::new(&mut check, 2, 2, 1, 1);
        exact.visit(1).unwrap();
        exact.visit(2).unwrap();
        assert_eq!(exact.visit(2).unwrap_err().code, "resource_limit");
        let mut check = || Ok(());
        let mut depth = ExtractionControl::new(&mut check, 2, 10, 1, 1);
        assert_eq!(depth.visit(3).unwrap_err().code, "resource_limit");
    }
}
