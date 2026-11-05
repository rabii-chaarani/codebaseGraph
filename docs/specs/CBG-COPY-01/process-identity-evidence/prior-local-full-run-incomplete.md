# Earlier broad local debug run — incomplete, not passing evidence

Command and state are recorded in prior-local-full-run-incomplete.json. Source was COD-52-only before the approved COPY instrumentation follow-up. The coordinator terminated the run before further source builds to preserve binary provenance.

606 completed pass records and one failure were observed before termination. Failure `dc_s5_real_containment_fifo_retires_with_pin_receipt_and_terminal_root` reports: “real containment qualification requires the captured shipping executable” at tests/support/storage_maintenance/collector.rs2020. The command lacked that fixture's captured-CLI prerequisite; this is not a reproduced product containment assertion failure. The failed fixture was retained by the harness. The later long20-update case had not completed at termination. No1110-case full passing result or complete JUnit is claimed.

Hosted CI40449278177 tested a tree-identical COD-52 source and passed the Mac native unit843case cohort (2ignored) plus Mac generation/integration/artifact, but failed the separate Windows COPY PID-cardinality assertion. Keep those results separate from this incomplete local command. For follow-up native process tests the coordinator freezes/captures the current CLI and supplies CARGO_BIN_EXE_codebase-graph; no prerequisite bypass or assertion weakening.
