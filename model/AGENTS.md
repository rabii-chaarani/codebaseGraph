# codebaseGraph SysML v2 model

Load all `.sysml` files under `model/` together in one SysML v2 project.
Start navigation at `model/system_model.sysml` (`CodebaseGraphModel`).
Cross-file references resolve through package imports. From this project root,
run `find model -name '*.sysml' -print0 | xargs -0 sysml -quiet -validate`
with OpenSysML, or `syside check model` with licensed Syside.

The model comes from the [SysML Architecture Proposal](https://chatgpt.com/c/6b03cc8a-f0c8-83ec-bddb-4bad3b85e65b)
conversation and its `CodebaseGraph_Redesign.sysml` baseline from the
Software Engineering workspace. The baseline models `rabii-chaarani/codebaseGraph`
at `90e2ab6fae2767bdb158e19f0072d43568e80dbc`.

Keep existing need and requirement IDs stable. Requirements live with
their originating use case. The system requirements package exposes both
sets through imports. Update affected need, use-case and verification
references when changing a requirement. Preserve the distinction between
draft decisions, agreed criteria and TBD values.

Verification cases describe planned checks. Record evidence separately when
those checks are performed. Detailed interfaces, subsystems, workflows and
state machines are deferred. The library packages are reserved for future
model increments. Product code and executable tests live in the existing
repository-level `src/` and `tests/` directories.
