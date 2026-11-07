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
        }
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

#[cfg(test)]
mod tests {
    use super::*;
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
