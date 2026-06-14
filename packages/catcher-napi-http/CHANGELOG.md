# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-napi-http-v0.3.13...catcher-napi-http-v1.0.0) (2026-06-14)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* A-01 priority queue + G-01/G-02 pool tuning + DNS host_mapping ([df9d26a](https://github.com/eric8810/catcher/commit/df9d26a285d8f3026f341c5da03ca93854f48498))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* built-in msgpack codec for HTTP and WS transports ([2f27c44](https://github.com/eric8810/catcher/commit/2f27c4422e790aefb8fd8499db539b8499df5cbf))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* complete remaining stubs — @catcher/web interceptors + WS client, napi-http JS fallback, remaining-work plan, env setup ([128041e](https://github.com/eric8810/catcher/commit/128041e2f7de06fe2c042bf1a176f25c3f316621))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* **dns:** implement StaleAwareDnsResolver for catcher-napi-http ([ac7cddc](https://github.com/eric8810/catcher/commit/ac7cddcd741def06c2f88f03606c2c358969da1d))
* keep napi http compatible with rust transport changes ([14b064f](https://github.com/eric8810/catcher/commit/14b064fb9ff6e80928d8a6a347d49c882f62421a))
* N-API wrapper 重命名为 client.js 避免 napi build 覆盖，修复 test 脚本 --include 语法，修复 pnpm pack 产物 ([3b0c009](https://github.com/eric8810/catcher/commit/3b0c009430863e6939f8c3dbc64fec9fbe05c22a))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* review bugs — per-request retry, onRetry double call, Cargo deps, nested workspace flatten, JSDoc fix, tsconfig for web ([bb87a02](https://github.com/eric8810/catcher/commit/bb87a022aac3ac97161864667138e70ec41307e3))
* stabilize napi e2e adapters ([a3ace13](https://github.com/eric8810/catcher/commit/a3ace1377039d6c13c54184e6546359fa5f4b21e))
* subagent 审查发现 — Web HTTP AbortError 误判、Web WS closedByUser 缺失、N-API 跨平台 fallback、根 build 脚本、.gitignore ([cca8bb0](https://github.com/eric8810/catcher/commit/cca8bb0208d5c86267ed3b55132725295e494aef))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))
