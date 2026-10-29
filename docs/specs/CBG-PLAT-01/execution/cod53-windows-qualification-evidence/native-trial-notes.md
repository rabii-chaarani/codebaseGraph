# Native prerequisite trial notes

- Official Ladybug0.19.0 static archive passed its published SHA256 check, but linking with installed MSVC14.39 failed with 10 unresolved newer STL symbols. This is a source-prerequisite compatibility failure, not an introduced default-feature defect.
- An attempted dependency source-mode retry reused stale external-library build output because upstream build.rs lacks environment invalidation for external selection. It was stopped; a fresh-target source trial was started then stopped before completion. No source-build pass/failure is claimed.
- Static source inspection briefly suspected the packaged missing WAL generator would block default build, but corrected review found its target is not ALL; generated source files are present. Python>=3.9 is still a configure prerequisite. No missing-generator default-build failure was executed.
- The next documented native route is LBUG_SHARED=1 with the exact official Ladybug0.19.0 shared headers/import lib/DLL. This preserves the dependency/ABI version. No upstream/product source patch, shim or version change.
