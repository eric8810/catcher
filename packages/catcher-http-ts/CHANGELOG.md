# Changelog

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher-http-v0.3.13...catcher-http-v0.4.0) (2026-06-14)


### Features

* add 98 unit tests for catcher-http-ts and catcher-ws-ts ([379b99e](https://github.com/eric8810/catcher/commit/379b99e9cff3a02b9e9496a5941bd3aa9fb09ed4))
* G2 rawData fix + TS client enhancements ([ffa723c](https://github.com/eric8810/catcher/commit/ffa723c544ab3b6e5f5798cdc3f1f539441167b1))
* implement G2-G12 API gap features across all layers ([60644dd](https://github.com/eric8810/catcher/commit/60644dd3c01091b7d211754aa65cbac38ca72e92))
* implement SSE streaming client (TS + Rust) ([5b8ba51](https://github.com/eric8810/catcher/commit/5b8ba51463d67ea124aeb7edb764eae92bc4bd2a))
* interceptor system + per-request options + FFI layering docs ([a17c810](https://github.com/eric8810/catcher/commit/a17c810c5ec9abd1ab44b01caf2f0f94306517e3))
* NEW-3 wire TS TLS config into Node.js https.Agent ([f325b05](https://github.com/eric8810/catcher/commit/f325b054f7d64255653ad74722dac44eb6d76770))
* wire DNS nameservers into TS CacheableLookup + mark G-10 fixed ([36e002a](https://github.com/eric8810/catcher/commit/36e002acb7a795660f4cb89dee9b13d8c0856908))


### Bug Fixes

* AbortError named export + high-concurrency throughput benchmark ([0566c05](https://github.com/eric8810/catcher/commit/0566c05193374064c57453b2435bd85ca2137036))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* review bugs — per-request retry, onRetry double call, Cargo deps, nested workspace flatten, JSDoc fix, tsconfig for web ([bb87a02](https://github.com/eric8810/catcher/commit/bb87a022aac3ac97161864667138e70ec41307e3))
* TLS默认安全、N-API加载、UniFFI构建、WS CloseEvent、maxAttempts、发布配置、web行为对齐、测试拆分 ([a5ed669](https://github.com/eric8810/catcher/commit/a5ed669148a8be1dd1c27b291f3350c5248bb004))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * @eric8810/catcher-core bumped to 0.4.0
  * devDependencies
    * @eric8810/catcher-core bumped to 0.4.0
