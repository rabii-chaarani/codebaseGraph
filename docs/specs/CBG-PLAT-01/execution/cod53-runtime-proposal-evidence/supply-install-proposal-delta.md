# COD-53 supply/install proposal delta

**Recommendation:** keep the full gate open for the concrete supply/install brief, not for a new architecture decision. Read `full-runtime-proposal.md`; no product writes/builds/tests/Git/tracking changes. Reviewer GPT-6.1 Sol High. Research2026-10-27.

## 1. Official package source is allowed in principle; “NuGet” is not a provider identity

The chosen Community2022 route does **not** require copying files from an Enterprise build host. Microsoft's VS2022 REDIST list expressly includes Community; its VC runtime section also says the same redistributable is available from Microsoft's supported download pages, subject to licensed-user/terms restrictions. Therefore a pinned **official Microsoft redistributable source**, with eligible unmodified release-x64 payloads and source/terms/hash evidence, can be used under the selected route. Installing the redistributable globally on the consumer is not required or permitted as a silent D01 replacement. Document a supported/reproducible extraction method and exact file provenance; source availability alone does not establish permission to extract/distribute arbitrary package internals.

No suitable official NuGet runtime package has been established. I inspected NuGet's live package metadata/owners and Microsoft's `Microsoft.VCRTForwarders.140` page/repository: that verified Microsoft package contains **forwarders and requires the underlying runtime already installed**, so it cannot meet D01. Packages with runtime-looking names and `authors=Microsoft` but third-party owners are not an official source. Do not use one merely because NuGet hosts it. If an actual Microsoft-owned full-runtime package is supplied, assess its exact payload/terms; no new owner policy decision is inherently needed for that source choice.

**Blocked commitment:** copying/packaging VC bytes waits for the actual identified provider/version, authorized eligible payloads and extraction/provenance method. The Community route choice is settled; do not ask it again. Actual entitlement remains the distribution owner's responsibility, not an inferred runner fact.

## 2. Direct VC imports do not mandate a versioned installer

If final EXEs directly import VC DLLs, those load before Rust; a nested private folder plus Rust preload cannot satisfy that initial import. Microsoft explicitly documents ordinary local deployment **beside the executable**. That is consistent with this draft and does not by itself require launchers, executable relocation or a versioned install directory. Do not redesign install layout before inspecting final actual EXE imports.

The real issue is safe upgrade of sibling DLLs in the current possibly shared BinDir. A candidate-only smoke does not prove old installed EXEs still work with changed sidecars. Microsoft documents v14 backward binary compatibility and requires a runtime at least as new as the build tools; that supports a conservative compatible-forward update, **not** arbitrary downgrades/mixed-runtime partial replacement or ownership of unrelated files. Settle a minimal rule: reuse identical files; reject unsupported/foreign existing sidecars or downgrade; validate permitted changed set against old and candidate executables; stage files before individual replacement and handle in-use/denied-copy failure without starting EXE replacement. Evidence must cover any intermediate sidecar set the installer can leave. Do not claim pair atomicity/power-loss rollback.

**No indispensable owner question yet:** sibling deployment can remain within A04 if that minimal compatible-forward/reuse-or-fail rule is feasible. If actual bytes/tests show an unavoidable destructive overwrite of a working installation, or the coordinator instead wants to relocate EXEs/add dispatch launchers/change user-visible paths or adopt new rollback guarantees, that is the concrete compatibility/layout decision to take to the user **before** that work. A versioned runtime folder cannot fix direct pre-main imports by itself.

## 3. Before full implementation, settle rather than add scaffolding

Select the provider/extraction evidence and one executable/sidecar upgrade rule in the Scryer brief. Also finish the draft's packaged/source-mode distinction, build-manifest generation order and narrowly scoped cache-input integrity contract. Those are real unfinished implementation premises; a new user decision is only needed if their concrete resolution changes approved behavior. Real Windows11 qualification remains later and does not invalidate the feasible local-deployment/preload design.

## Primary sources actually inspected

- VS2022 REDIST list, Community applicability and VC runtime downloadable-source terms: https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution
- Microsoft local deployment (DLLs beside application): https://learn.microsoft.com/en-us/cpp/windows/deployment-in-visual-cpp?view=msvc-170
- v14 compatibility/minimum runtime version: https://learn.microsoft.com/en-us/cpp/porting/binary-compat-2015-2017?view=msvc-170 and https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170
- Actual Microsoft forwarder package and source README, not a complete runtime provider: https://www.nuget.org/packages/Microsoft.VCRTForwarders.140/1.1.0 and https://github.com/microsoft/vcrt-forwarders . Live NuGet query API metadata inspected for package owners.

Search-service discovery was unavailable (no configured key); the above primary sources/API were fetched directly. No suitable full-runtime NuGet provider absence is claimed beyond the packages inspected. No constants, runtime bytes, install layout or permission facts changed.
