# CBG-COPY-01 — Resource-ceiling amendment approval

- Approved canonical amendment: [resource-ceilings-amendment.md](resource-ceilings-amendment.md), revision R1.
- Full specification commit: `c8dd4ea9ca2a6f992756a2a41828396a27eabb6d`.
- File SHA-256: `40fa2007539bf26e13a88a3c894428e3cee9dc46230d647e4be6bc1c2ca9b696`.
- User decision, this conversation, 2026-10-26: **“remove only the enforcement and options”**, answering whether to remove hard resource enforcement/options while retaining streaming/chunking, bounded diagnostics and ownership/containment.
- Implementation applies after PR153 head `8e47d2b3c74b72b56a0b6b212d1bf939c8088ca7`; update existing PR153, do not merge/deploy. Earlier Scope A approval stays intact except the hard-workload-budget requirements explicitly amended here.
- Scryer `chg-vhbvd3` records changed existing intent and the real same-scope user sign-off. Linear remains unavailable pending verification; no synchronization is claimed.

Independent review and executed qualification remain separate from this actual user approval. Removing application workload enforcement does not remove physical OS/native limits or correctness/security/cleanup bounds.
