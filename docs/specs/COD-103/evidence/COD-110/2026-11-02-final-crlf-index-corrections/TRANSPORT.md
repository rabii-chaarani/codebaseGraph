# Evidence transport

Two unpacked evidence subtrees are now inert, uncompressed TAR carriers so the
complete checkout fits the observed Windows root (33 characters including its
trailing separator) without
changing Git long-path settings. No workflow or test bypass is introduced.

`carrier-map.json` maps each original repository-relative logical path to a
repository-relative carrier and a member relative to the former directory.
`carrier-files.json` records the new physical files, sizes, modes and SHA-256
hashes; it excludes itself to avoid a self-referential hash.

The historical `files.json` manifests, closed reports, canonical specifications
and approvals are unchanged. Their paths and hashes describe the original logical
payloads, not the current unpacked physical layout. For a transported path, read
the named member from its carrier and verify its original size and SHA-256 hash.
Do not extract the carriers into the checkout; use a separate short scratch root
if physical extraction is needed. The TARs contain only unique, safe relative
regular-file members with original mode 0644 and byte-identical payloads. Archive
owner and timestamp fields are normalized to zero; payload bytes are unchanged.

The run-logs carrier preserves 178 files / 13,780,767 payload bytes. The local
Windows baseline carrier preserves 22 files / 940,943 payload bytes, including
exactly 10 zero-byte files. All 200 members were compared against the reachable
source commit `8f8838a06fab1aafc91962f306924a92c7a5b806` and the immutable prior
member inventories, then verified again by reading the completed carriers.

This transport correction does not revise historical outcomes or constitute new
platform qualification. Independent full-tree checkout and affected-behavior
checks on macOS and Windows, focused final review, and delivery authorization
remain separate gates.
