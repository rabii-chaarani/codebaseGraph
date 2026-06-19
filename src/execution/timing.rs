use std::time::Instant;

pub(super) fn elapsed_seconds(started: Instant) -> f64 {
    started.elapsed().as_secs_f64()
}
