# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-core-v0.3.13...catcher-core-v1.0.0) (2026-06-14)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* A-01 priority queue + G-01/G-02 pool tuning + DNS host_mapping ([df9d26a](https://github.com/eric8810/catcher/commit/df9d26a285d8f3026f341c5da03ca93854f48498))
* implement SSE streaming client (TS + Rust) ([5b8ba51](https://github.com/eric8810/catcher/commit/5b8ba51463d67ea124aeb7edb764eae92bc4bd2a))
* N-02/N-03/N-04 native layer capability gaps ([e7dc98f](https://github.com/eric8810/catcher/commit/e7dc98f47e27711c5b3ab8782279bbfecd87c8e2))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* critical review issues — use-after-free, async UniFFI, timeout race ([03753f0](https://github.com/eric8810/catcher/commit/03753f0a33f5ef38d2235a1c4cbfd72a0da39ce3))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
