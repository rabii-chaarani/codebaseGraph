# Isolated instruction walkthrough

Everything below is fictitious. Use the provided engineer skill and supplied authoritative rules to respond to the user request. Do not call real integrations, launch real workers, mutate a repository or create real tracking. Express tool/worker actions as mock requests and responses. All issue/spec/commit/source identifiers are synthetic. A mock result is not production evidence. Each variant starts independently unless marked as a continuation. Only the fixture input and its source files go to the participant; grading material is separate.

# Entry and rules

## Variant A
User: “Implement CBG-MOCK-10.”
The delivery checkout is `/mock/codebaseGraph-engineer` at `1111111111111111111111111111111111111111`. It and its ancestors contain no AGENTS.md. No other authoritative rules location has been supplied. A different dirty checkout `/mock/codebaseGraph-original` has an AGENTS.md, and an old chat summary says it was used previously. CBG-MOCK-10 is marked Ready. No implementation has started.

## Variant B
User: “Read-only: explain why CBG-MOCK-11 remains blocked; do not change anything.”
Authoritative rules have been supplied read-only at `/mock/policies/AGENTS.md`; their content is the supplied repository rules. The verified mock CodebaseGraph workspace shows CBG-MOCK-11 in scope `Parser / M2`, blocked by CBG-MOCK-9. Canonical spec `docs/specs/MOCK-11/spec.md` revision 1 is committed but has no user approval record. Scryer brief `mock-brief-11` requires the shared contract from CBG-MOCK-9; that contract is not implemented or qualified. No source reading is needed to explain these records. Linear and Scryer are readable; writes would be available if requested.
