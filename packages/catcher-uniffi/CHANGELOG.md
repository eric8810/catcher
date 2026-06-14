# Changelog

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher-uniffi-v0.3.13...catcher-uniffi-v0.4.0) (2026-06-14)


### Features

* A-01 priority queue + G-01/G-02 pool tuning + DNS host_mapping ([df9d26a](https://github.com/eric8810/catcher/commit/df9d26a285d8f3026f341c5da03ca93854f48498))
* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))


### Bug Fixes

* [#023](https://github.com/eric8810/catcher/issues/023) uniffi evaluate_quality take/put race ([060b2ce](https://github.com/eric8810/catcher/commit/060b2ce065c0e813e083dc312077e73af33394a4))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* critical review issues — use-after-free, async UniFFI, timeout race ([03753f0](https://github.com/eric8810/catcher/commit/03753f0a33f5ef38d2235a1c4cbfd72a0da39ce3))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))
* UniFFI setup_scaffolding, add catcher_free_result, remove UDL ([1da6f30](https://github.com/eric8810/catcher/commit/1da6f303be47667c9ec943db6f8babedfa2158d1))
* **uniffi:** use uniffi::setup_scaffolding!() instead of uniffi::setup!() ([fad2ab8](https://github.com/eric8810/catcher/commit/fad2ab8f568e6579ebf39f65acfcbe02df4b8cc6))
* wire Flutter FFI calls, implement UniFFI WsClient, fix test scripts, add CI/release infra ([670f915](https://github.com/eric8810/catcher/commit/670f915c042ea28928fd2e6b0d413d27cb75693d))
