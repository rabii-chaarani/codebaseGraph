mod accumulator;
mod connectors;
mod files;
mod merge;
mod ordering;
mod result;
mod rows;

pub(crate) use accumulator::StagingAccumulator;
pub(crate) use result::StagingResult;

#[cfg(test)]
mod tests;
