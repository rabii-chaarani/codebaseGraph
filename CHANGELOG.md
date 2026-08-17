# Changelog

## [1.4.5](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.4.4...v1.4.5) (2026-08-17)


### Bug Fixes

* **release:** bind asset upload repository ([#82](https://github.com/rabii-chaarani/codebaseGraph/issues/82)) ([2a898c6](https://github.com/rabii-chaarani/codebaseGraph/commit/2a898c6f0c50345723d0d60cf9fc8df0da8f23ee))

## [1.4.4](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.4.3...v1.4.4) (2026-08-17)


### Bug Fixes

* **release:** publish after skipped rebuild ([#80](https://github.com/rabii-chaarani/codebaseGraph/issues/80)) ([392b26c](https://github.com/rabii-chaarani/codebaseGraph/commit/392b26cfc304732489cdbf66986ab3d37b310a84))

## [1.4.3](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.4.2...v1.4.3) (2026-08-17)


### Bug Fixes

* **release:** prevent stale tag publication ([#77](https://github.com/rabii-chaarani/codebaseGraph/issues/77)) ([1931dc8](https://github.com/rabii-chaarani/codebaseGraph/commit/1931dc85d32f92477f2fa244dbebb3e0f5ea4c44))

## [1.4.2](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.4.1...v1.4.2) (2026-08-17)


### Bug Fixes

* **ci:** repair crates.io package verification ([#75](https://github.com/rabii-chaarani/codebaseGraph/issues/75)) ([5f2686a](https://github.com/rabii-chaarani/codebaseGraph/commit/5f2686af7c83bb7127072739602efc116be2142f))

## [1.4.1](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.4.0...v1.4.1) (2026-08-14)


### Bug Fixes

* **release:** trigger publication from completed ci ([#73](https://github.com/rabii-chaarani/codebaseGraph/issues/73)) ([8d9cc44](https://github.com/rabii-chaarani/codebaseGraph/commit/8d9cc44e77d8cb5f3bef24a4dd6e550a24aba9ad))

## [1.4.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.3.1...v1.4.0) (2026-08-13)


### Features

* add language grammar extractors ([#65](https://github.com/rabii-chaarani/codebaseGraph/issues/65)) ([ee3a7bd](https://github.com/rabii-chaarani/codebaseGraph/commit/ee3a7bd4e352c0608a9ae589de110896e9bbc73d))

## [1.3.1](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.3.0...v1.3.1) (2026-08-11)


### Bug Fixes

* **memory:** enhance agent-memory workflow documentation and add distinct memory kind tests ([#63](https://github.com/rabii-chaarani/codebaseGraph/issues/63)) ([a447ac3](https://github.com/rabii-chaarani/codebaseGraph/commit/a447ac3a3fad28762dfb33f51fc971afc941b8bf))

## [1.3.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.2.2...v1.3.0) (2026-08-10)


### Features

* **k-wiki:** initialize memory source on install ([#61](https://github.com/rabii-chaarani/codebaseGraph/issues/61)) ([6680d59](https://github.com/rabii-chaarani/codebaseGraph/commit/6680d5931584d8817b8b7adc9b210a6920667e2a))

## [1.2.2](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.2.1...v1.2.2) (2026-08-07)


### Features

* **k-wiki:** make repository memory durable and reviewable ([cbcbc71](https://github.com/rabii-chaarani/codebaseGraph/commit/cbcbc719cc9df462b4bd57593560517be6d2c73a))
* **storage:** prevent graph growth with immutable generations ([e4f5c95](https://github.com/rabii-chaarani/codebaseGraph/commit/e4f5c95d62a7f8c318cac80b863844c17938ac6f))


### Bug Fixes

* **ci:** compare escaped Windows MCP commands ([0ba9f7e](https://github.com/rabii-chaarani/codebaseGraph/commit/0ba9f7ee8f3cbed2b90a8a87ddd144932809bfbc))
* **ci:** make storage cleanup and audit checks portable ([81b699b](https://github.com/rabii-chaarani/codebaseGraph/commit/81b699ba404978c685047f36a3cf5caf24f9e7d0))
* **ci:** make Windows lock and Hermes tests portable ([515a43c](https://github.com/rabii-chaarani/codebaseGraph/commit/515a43c4bdebdcf48bf8ea67e3ca906500be936a))
* **ci:** prevent Windows test shard stalls ([89cb0f6](https://github.com/rabii-chaarani/codebaseGraph/commit/89cb0f6f693096f3290c55b2b039cb9858650a73))
* **k-wiki:** manage memory guidance separately ([f82659d](https://github.com/rabii-chaarani/codebaseGraph/commit/f82659de1d8ed7bc9f5c95fc6f6587a3072f3446))
* **k-wiki:** prevent cross-repository MCP routing ([6fc7bbf](https://github.com/rabii-chaarani/codebaseGraph/commit/6fc7bbf3b3dad629ab2ccb4bcb92d9e23a641865))
* **k-wiki:** prevent cross-repository wiki routing ([87898aa](https://github.com/rabii-chaarani/codebaseGraph/commit/87898aa634c9e6ad7b2c730280bc6975bfa4881e))
* **release:** restore release workflow execution ([#58](https://github.com/rabii-chaarani/codebaseGraph/issues/58)) ([1c3c481](https://github.com/rabii-chaarani/codebaseGraph/commit/1c3c481700cbb8ec503e481c4b58ed09db97faa8))
* **storage:** avoid unsupported Windows directory sync ([3376c22](https://github.com/rabii-chaarani/codebaseGraph/commit/3376c2249abf2b200d359e6faf4f98d07f8bfe2f))
* **storage:** release refresh leases before watcher idle ([096ed2e](https://github.com/rabii-chaarani/codebaseGraph/commit/096ed2e44e24b689eedc65750ad1a91f5723dd20))
* **storage:** reuse portable artifact directory sync ([f092d77](https://github.com/rabii-chaarani/codebaseGraph/commit/f092d7769018d901c27dd12759ac4e1a9c025801))
* **storage:** sync published generations portably ([df00bc3](https://github.com/rabii-chaarani/codebaseGraph/commit/df00bc3fc41c5a9eb9fbe4259b02a591584bb024))

## [1.2.1](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.2.0...v1.2.1) (2026-08-05)


### Bug Fixes

* **ci:** unblock Windows release linking ([#53](https://github.com/rabii-chaarani/codebaseGraph/issues/53)) ([0edad59](https://github.com/rabii-chaarani/codebaseGraph/commit/0edad5908fd34503cf54a8097bcb9eeb8e0b4f12))

## [1.2.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.6...v1.2.0) (2026-08-04)


### Features

* Implement the k-wiki knowledge management subsystem ([#51](https://github.com/rabii-chaarani/codebaseGraph/issues/51)) ([fd8fe8e](https://github.com/rabii-chaarani/codebaseGraph/commit/fd8fe8e5b1ae63d59f53b0b2825ccae503485ef8))

## [1.1.6](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.5...v1.1.6) (2026-06-23)


### Bug Fixes

* Recover live graph refresh after DB contention ([#40](https://github.com/rabii-chaarani/codebaseGraph/issues/40)) ([ee1754b](https://github.com/rabii-chaarani/codebaseGraph/commit/ee1754b66f98a69832b72dbd21224e7a87e97b19))

## [1.1.5](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.4...v1.1.5) (2026-06-22)


### Bug Fixes

* **release:** shrink crates.io package ([07c2e3c](https://github.com/rabii-chaarani/codebaseGraph/commit/07c2e3cc17a0e68593335373cf2a3f11b8964904))

## [1.1.4](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.3...v1.1.4) (2026-06-22)


### Bug Fixes

* **ci:** use bash for release artifact upload ([276606c](https://github.com/rabii-chaarani/codebaseGraph/commit/276606c005b3e9e59b5cfee9f36bf8e943bdd859))

## [1.1.3](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.2...v1.1.3) (2026-06-22)


### Bug Fixes

* **ci:** use portable release checksums ([54daa47](https://github.com/rabii-chaarani/codebaseGraph/commit/54daa476202bdbd352ca2a6753a2b836b3bca4f9))

## [1.1.2](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.1...v1.1.2) (2026-06-22)


### Bug Fixes

* **ci:** use cargo release environment ([ede151f](https://github.com/rabii-chaarani/codebaseGraph/commit/ede151f8a52897d58f753a9dbed8190f6bdb10f9))

## [1.1.1](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.1.0...v1.1.1) (2026-06-22)


### Bug Fixes

* **ci:** read release confirmations at step scope ([c1000de](https://github.com/rabii-chaarani/codebaseGraph/commit/c1000de52a600d0a1a9ba31756d3b25bbc49f5c2))

## [1.1.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v1.0.0...v1.1.0) (2026-06-22)


### Features

* reduce graph build latency with parallel defaults ([#33](https://github.com/rabii-chaarani/codebaseGraph/issues/33)) ([4a2af02](https://github.com/rabii-chaarani/codebaseGraph/commit/4a2af027070950c02325cfab6a6326ab157cb7cf))

## [1.0.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.5.0...v1.0.0) (2026-06-19)


### ⚠ BREAKING CHANGES

* Rust version of codebaseGraph ([#26](https://github.com/rabii-chaarani/codebaseGraph/issues/26))

### Features

* Rust version of codebaseGraph ([#26](https://github.com/rabii-chaarani/codebaseGraph/issues/26)) ([712c757](https://github.com/rabii-chaarani/codebaseGraph/commit/712c75724c4bf353904a537bba1a4894c6969551))

## [0.5.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.4.0...v0.5.0) (2026-06-15)


### Features

* Enhance semantic resolution across ingestion and reference handling ([#23](https://github.com/rabii-chaarani/codebaseGraph/issues/23)) ([5bad710](https://github.com/rabii-chaarani/codebaseGraph/commit/5bad71071f3fcb0de085d826ad872d64a5628555))

## [0.4.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.3.0...v0.4.0) (2026-06-12)


### Features

* parse supported languages by default ([#21](https://github.com/rabii-chaarani/codebaseGraph/issues/21)) ([154006f](https://github.com/rabii-chaarani/codebaseGraph/commit/154006fdbae21b903f41a7bfe5df75d4d02f2f4f))

## [0.3.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.5...v0.3.0) (2026-06-12)


### Features

* Extend ontology and setup workflows for language profiles ([#19](https://github.com/rabii-chaarani/codebaseGraph/issues/19)) ([6c684ea](https://github.com/rabii-chaarani/codebaseGraph/commit/6c684ea71208c9d847ca04a80f360c3de30d9ff3))

## [0.2.5](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.4...v0.2.5) (2026-06-12)


### Bug Fixes

* improved codebase context output ([#17](https://github.com/rabii-chaarani/codebaseGraph/issues/17)) ([54cd8ee](https://github.com/rabii-chaarani/codebaseGraph/commit/54cd8eee0d2dc1c41fde6c64a2667c53cff9d817))

## [0.2.4](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.3...v0.2.4) (2026-06-10)


### Documentation

* document runtime code paths ([#14](https://github.com/rabii-chaarani/codebaseGraph/issues/14)) ([c302b2a](https://github.com/rabii-chaarani/codebaseGraph/commit/c302b2ac13c19929e00f05666d20bb6a30abcd1c))

## [0.2.3](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.2...v0.2.3) (2026-06-10)


### Bug Fixes

* ignore local Scryer state ([#13](https://github.com/rabii-chaarani/codebaseGraph/issues/13)) ([098bbc8](https://github.com/rabii-chaarani/codebaseGraph/commit/098bbc8815af3947f2bc4d4d7aa41e787a67aa24))

## [0.2.2](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.1...v0.2.2) (2026-06-09)


### Bug Fixes

* [codex] mcp tools exposed ([#10](https://github.com/rabii-chaarani/codebaseGraph/issues/10)) ([2f3c793](https://github.com/rabii-chaarani/codebaseGraph/commit/2f3c7936a4fad97ceba9d17d5e77e616de5f73ef))

## [0.2.1](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.2.0...v0.2.1) (2026-05-28)


### Bug Fixes

* **release:** download release assets without git checkout ([#7](https://github.com/rabii-chaarani/codebaseGraph/issues/7)) ([05d046d](https://github.com/rabii-chaarani/codebaseGraph/commit/05d046dbabccb391e66ae8ec40371613e5b59568))

## [0.2.0](https://github.com/rabii-chaarani/codebaseGraph/compare/v0.1.0...v0.2.0) (2026-05-28)


### Features

* added source code ([ba29182](https://github.com/rabii-chaarani/codebaseGraph/commit/ba29182aa3c8055fd9c476be41ac01a32cc81cc2))
* full functioning knowledge graph ([ee1dac5](https://github.com/rabii-chaarani/codebaseGraph/commit/ee1dac56f0b7f5152c865fa2bf1ba3c1e30f7c76))


### Bug Fixes

* **release:** enforce strict semver release tags ([#5](https://github.com/rabii-chaarani/codebaseGraph/issues/5)) ([f0f9342](https://github.com/rabii-chaarani/codebaseGraph/commit/f0f9342320cb6066ee1f95163d144709306efd67))

## 0.1.0 (2026-05-28)


### Features

* added source code ([ba29182](https://github.com/rabii-chaarani/codebaseGraph/commit/ba29182aa3c8055fd9c476be41ac01a32cc81cc2))
* full functioning knowledge graph ([ee1dac5](https://github.com/rabii-chaarani/codebaseGraph/commit/ee1dac56f0b7f5152c865fa2bf1ba3c1e30f7c76))

## Changelog

Release notes are managed by release-please.
