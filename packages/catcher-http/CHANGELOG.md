# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-http-v0.3.8...catcher-http-v1.0.0) (2026-05-19)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* A-01 priority queue + G-01/G-02 pool tuning + DNS host_mapping ([df9d26a](https://github.com/eric8810/catcher/commit/df9d26a285d8f3026f341c5da03ca93854f48498))
* B-02 multipart/form-data encoder (Rust) + fix FFI callback type ([6c35bee](https://github.com/eric8810/catcher/commit/6c35bee393bdb7573539e2e6610dec43b35636ad))
* G-11 reporter stats fix + NEW-2 web progress + Rust DNS nameservers ([ade4bf2](https://github.com/eric8810/catcher/commit/ade4bf24d47fd0574cecb34604fa13fcdb011946))
* implement G2-G12 API gap features across all layers ([60644dd](https://github.com/eric8810/catcher/commit/60644dd3c01091b7d211754aa65cbac38ca72e92))
* implement SSE streaming client (TS + Rust) ([5b8ba51](https://github.com/eric8810/catcher/commit/5b8ba51463d67ea124aeb7edb764eae92bc4bd2a))
* N-02/N-03/N-04 native layer capability gaps ([e7dc98f](https://github.com/eric8810/catcher/commit/e7dc98f47e27711c5b3ab8782279bbfecd87c8e2))
* NEW-1 pin_sha256 cert pinning + G-06 proxy bandwidth fix ([95a19ad](https://github.com/eric8810/catcher/commit/95a19ad0e6be567167117c7384aa74ddf3cf8b7d))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* [#022](https://github.com/eric8810/catcher/issues/022) stream chunk base64, [#023](https://github.com/eric8810/catcher/issues/023) quality race, [#024](https://github.com/eric8810/catcher/issues/024) SSE block_on panic ([9ba65a8](https://github.com/eric8810/catcher/commit/9ba65a84bac67e5eb31bc8a4dca7fb349605cfa9))
* **catcher-http:** wire http_retries metric via custom MetricsRetryMiddleware ([2294f26](https://github.com/eric8810/catcher/commit/2294f267b1c86507930b47302c4b2dedbd0d5a4b))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* critical review issues — use-after-free, async UniFFI, timeout race ([03753f0](https://github.com/eric8810/catcher/commit/03753f0a33f5ef38d2235a1c4cbfd72a0da39ce3))
* FFI body base64 encoding ([#019](https://github.com/eric8810/catcher/issues/019), [#021](https://github.com/eric8810/catcher/issues/021)) + adaptive heartbeat timer ([#020](https://github.com/eric8810/catcher/issues/020)) ([501c55b](https://github.com/eric8810/catcher/commit/501c55b715d53b623f100e2beb8563740c61c8b6))
* FFI HttpError as response JSON + Dart body_base64/data_base64 compat ([04d03a3](https://github.com/eric8810/catcher/commit/04d03a37b4a4dd8667fd72b68ec796536bd7912b))
* keep napi http compatible with rust transport changes ([14b064f](https://github.com/eric8810/catcher/commit/14b064fb9ff6e80928d8a6a347d49c882f62421a))
* resolve 4 issues — mem leak, SSE O(n²), P90 perf, config clone ([#001](https://github.com/eric8810/catcher/issues/001) [#003](https://github.com/eric8810/catcher/issues/003) [#004](https://github.com/eric8810/catcher/issues/004) [#005](https://github.com/eric8810/catcher/issues/005)) ([5f4a950](https://github.com/eric8810/catcher/commit/5f4a9502c4a30276428eb479fcc0236c217c2256))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* **sse:** move readyState reset to loop top, fix RC5 timing issue ([57771a2](https://github.com/eric8810/catcher/commit/57771a229d2f8a07742c06ab112f49e6eb14216e))
* **test:** resolve two CI test failures ([7c9aa8a](https://github.com/eric8810/catcher/commit/7c9aa8a49182b2c9bb7cd6eed550b5e4884297a5))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))
* wire Flutter FFI calls, implement UniFFI WsClient, fix test scripts, add CI/release infra ([670f915](https://github.com/eric8810/catcher/commit/670f915c042ea28928fd2e6b0d413d27cb75693d))
