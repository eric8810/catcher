# Changelog

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher-web-v0.3.18...catcher-web-v0.4.0) (2026-08-13)


### Features

* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* complete remaining stubs — @catcher/web interceptors + WS client, napi-http JS fallback, remaining-work plan, env setup ([128041e](https://github.com/eric8810/catcher/commit/128041e2f7de06fe2c042bf1a176f25c3f316621))
* G-11 reporter stats fix + NEW-2 web progress + Rust DNS nameservers ([ade4bf2](https://github.com/eric8810/catcher/commit/ade4bf24d47fd0574cecb34604fa13fcdb011946))
* G2 rawData fix + TS client enhancements ([ffa723c](https://github.com/eric8810/catcher/commit/ffa723c544ab3b6e5f5798cdc3f1f539441167b1))
* implement G2-G12 API gap features across all layers ([60644dd](https://github.com/eric8810/catcher/commit/60644dd3c01091b7d211754aa65cbac38ca72e92))
* implement SSE streaming client (TS + Rust) ([5b8ba51](https://github.com/eric8810/catcher/commit/5b8ba51463d67ea124aeb7edb764eae92bc4bd2a))


### Bug Fixes

* **flutter:** restore WebSocket connection and prevent startup crash ([#21](https://github.com/eric8810/catcher/issues/21)) ([9882a08](https://github.com/eric8810/catcher/commit/9882a084dec0361ad7c49dfc564aeddb144d2b03))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* review bugs — per-request retry, onRetry double call, Cargo deps, nested workspace flatten, JSDoc fix, tsconfig for web ([bb87a02](https://github.com/eric8810/catcher/commit/bb87a022aac3ac97161864667138e70ec41307e3))
* subagent 审查发现 — Web HTTP AbortError 误判、Web WS closedByUser 缺失、N-API 跨平台 fallback、根 build 脚本、.gitignore ([cca8bb0](https://github.com/eric8810/catcher/commit/cca8bb0208d5c86267ed3b55132725295e494aef))
* **test:** resolve two CI test failures ([7c9aa8a](https://github.com/eric8810/catcher/commit/7c9aa8a49182b2c9bb7cd6eed550b5e4884297a5))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * @eric8810/catcher-core bumped to 0.4.0
