# Changelog

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher-ffi-v0.3.13...catcher-ffi-v0.4.0) (2026-06-14)


### Features

* catcher-ffi umbrella crate + real FFI integration tests ([97a5b1e](https://github.com/eric8810/catcher/commit/97a5b1eebed3806486e62a3a8c52ed85a928a231))
* N-02/N-03/N-04 native layer capability gaps ([e7dc98f](https://github.com/eric8810/catcher/commit/e7dc98f47e27711c5b3ab8782279bbfecd87c8e2))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))


### Bug Fixes

* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* **ffi:** encode registry id directly in handles to eliminate use-after-free ([2cd7de5](https://github.com/eric8810/catcher/commit/2cd7de5fa2d54e3fa0a8675f110304dd8ad89504))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
