# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-napi-ws-v0.3.11...catcher-napi-ws-v1.0.0) (2026-06-01)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* built-in msgpack codec for HTTP and WS transports ([2f27c44](https://github.com/eric8810/catcher/commit/2f27c4422e790aefb8fd8499db539b8499df5cbf))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Dart FFI bindings + napi-ws types + CI update ([39ac501](https://github.com/eric8810/catcher/commit/39ac501c190a67b06ea311cd06af8ac9e1dabbc9))
* **napi-ws:** expose pack/unpack + add agent & codec benchmarks ([54bba24](https://github.com/eric8810/catcher/commit/54bba2437cfca7b058fd6704ac37e19bd610ed52))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* **dart:** wire dns and msgpack config ([d419a36](https://github.com/eric8810/catcher/commit/d419a36fb1f350b6e92d6b1e2c04eaca0ac60496))
* FFI body base64 encoding ([#019](https://github.com/eric8810/catcher/issues/019), [#021](https://github.com/eric8810/catcher/issues/021)) + adaptive heartbeat timer ([#020](https://github.com/eric8810/catcher/issues/020)) ([501c55b](https://github.com/eric8810/catcher/commit/501c55b715d53b623f100e2beb8563740c61c8b6))
* N-API wrapper 重命名为 client.js 避免 napi build 覆盖，修复 test 脚本 --include 语法，修复 pnpm pack 产物 ([3b0c009](https://github.com/eric8810/catcher/commit/3b0c009430863e6939f8c3dbc64fec9fbe05c22a))
* stabilize napi e2e adapters ([a3ace13](https://github.com/eric8810/catcher/commit/a3ace1377039d6c13c54184e6546359fa5f4b21e))
* subagent 审查发现 — Web HTTP AbortError 误判、Web WS closedByUser 缺失、N-API 跨平台 fallback、根 build 脚本、.gitignore ([cca8bb0](https://github.com/eric8810/catcher/commit/cca8bb0208d5c86267ed3b55132725295e494aef))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))
