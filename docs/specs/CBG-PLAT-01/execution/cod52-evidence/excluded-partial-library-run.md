# Excluded partial library run

The full library run `library-full.log` began before the candidate receipt test was integrated. A later targeted Cargo build reused the same target/test binary path while that run remained active. The coordinator terminated it to avoid mixed-binary provenance. Preserve the partial output as excluded historical evidence; it is not a passing or source-specific full-suite result. Final full-library verification must run only after source/test edits and builds are serialized.
