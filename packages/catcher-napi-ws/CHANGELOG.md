# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-napi-ws-v0.3.19...catcher-napi-ws-v1.0.0) (2026-08-13)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* built-in msgpack codec for HTTP and WS transports ([2f27c44](https://github.com/eric8810/catcher/commit/2f27c4422e790aefb8fd8499db539b8499df5cbf))
* **catcher-ws:** add send_timeout_ms so half-open sends cannot stall the event loop ([88d36e0](https://github.com/eric8810/catcher/commit/88d36e019cd05a160f3defdd3d9215bc3f87eceb))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Dart FFI bindings + napi-ws types + CI update ([39ac501](https://github.com/eric8810/catcher/commit/39ac501c190a67b06ea311cd06af8ac9e1dabbc9))
* **napi-ws:** expose pack/unpack + add agent & codec benchmarks ([54bba24](https://github.com/eric8810/catcher/commit/54bba2437cfca7b058fd6704ac37e19bd610ed52))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))
* **proxy:** add explicit direct mode ([#24](https://github.com/eric8810/catcher/issues/24)) ([07fe30a](https://github.com/eric8810/catcher/commit/07fe30a5514f30dfe68dcc3b234e61e1d1eef22f))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* **bindings:** align old-addon guards and error messages ([0ad6380](https://github.com/eric8810/catcher/commit/0ad6380240da882061c640cf38b5be06d48e75c5))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* **dart:** wire dns and msgpack config ([d419a36](https://github.com/eric8810/catcher/commit/d419a36fb1f350b6e92d6b1e2c04eaca0ac60496))
* FFI body base64 encoding ([#019](https://github.com/eric8810/catcher/issues/019), [#021](https://github.com/eric8810/catcher/issues/021)) + adaptive heartbeat timer ([#020](https://github.com/eric8810/catcher/issues/020)) ([501c55b](https://github.com/eric8810/catcher/commit/501c55b715d53b623f100e2beb8563740c61c8b6))
* **flutter:** restore WebSocket connection and prevent startup crash ([#21](https://github.com/eric8810/catcher/issues/21)) ([9882a08](https://github.com/eric8810/catcher/commit/9882a084dec0361ad7c49dfc564aeddb144d2b03))
* N-API wrapper 重命名为 client.js 避免 napi build 覆盖，修复 test 脚本 --include 语法，修复 pnpm pack 产物 ([3b0c009](https://github.com/eric8810/catcher/commit/3b0c009430863e6939f8c3dbc64fec9fbe05c22a))
* **napi:** enable system-proxy feature in bindings and sync ProxyConfig TS types ([#19](https://github.com/eric8810/catcher/issues/19)) ([add3c0a](https://github.com/eric8810/catcher/commit/add3c0a5df8def30c690e2c25b5c01f1728690bf))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* stabilize napi e2e adapters ([a3ace13](https://github.com/eric8810/catcher/commit/a3ace1377039d6c13c54184e6546359fa5f4b21e))
* subagent 审查发现 — Web HTTP AbortError 误判、Web WS closedByUser 缺失、N-API 跨平台 fallback、根 build 脚本、.gitignore ([cca8bb0](https://github.com/eric8810/catcher/commit/cca8bb0208d5c86267ed3b55132725295e494aef))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))
