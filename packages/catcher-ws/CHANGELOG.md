# Changelog

## [1.0.0](https://github.com/eric8810/catcher/compare/catcher-ws-v0.3.13...catcher-ws-v1.0.0) (2026-06-14)


### ⚠ BREAKING CHANGES

* napi package entry points moved from client.js to dist/client.js

### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* built-in msgpack codec for HTTP and WS transports ([2f27c44](https://github.com/eric8810/catcher/commit/2f27c4422e790aefb8fd8499db539b8499df5cbf))
* **catcher-ws:** add network_changed() for immediate reconnect on network switch ([b569f28](https://github.com/eric8810/catcher/commit/b569f284b5852c0824cce759f8a0773fc1ba2cab))
* **catcher-ws:** add send_timeout_ms so half-open sends cannot stall the event loop ([88d36e0](https://github.com/eric8810/catcher/commit/88d36e019cd05a160f3defdd3d9215bc3f87eceb))
* Phase 1+2 — N-02 streaming, N-03 per-request cancel, N-04 quality push, NAPI 10-gap, WS full config integration ([b314636](https://github.com/eric8810/catcher/commit/b314636b2c8f052e77aab0aa11ad3ac6a38e6737))
* replace hand-written napi wrappers with typed TS sources ([b075b37](https://github.com/eric8810/catcher/commit/b075b37f658d7a4d2ec72233551aea9fbf4b93d0))


### Bug Fixes

* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* **catcher-ws:** harden network_changed edge cases found in review ([0cabf7a](https://github.com/eric8810/catcher/commit/0cabf7ab350b24c5d02503fa51a756d52ee53a68))
* **ci:** repair release workflow — add missing dep versions, fix napi CLI path, seed release-please manifest ([1607f0c](https://github.com/eric8810/catcher/commit/1607f0c94b1e11d0610a0e196a049c97ed4182ec))
* critical review issues — use-after-free, async UniFFI, timeout race ([03753f0](https://github.com/eric8810/catcher/commit/03753f0a33f5ef38d2235a1c4cbfd72a0da39ce3))
* **dart:** wire dns and msgpack config ([d419a36](https://github.com/eric8810/catcher/commit/d419a36fb1f350b6e92d6b1e2c04eaca0ac60496))
* double-free in catcher_free_result ([13e07a7](https://github.com/eric8810/catcher/commit/13e07a7a5d65e3aa2fbbb3897d2111a4adc6e926))
* FFI body base64 encoding ([#019](https://github.com/eric8810/catcher/issues/019), [#021](https://github.com/eric8810/catcher/issues/021)) + adaptive heartbeat timer ([#020](https://github.com/eric8810/catcher/issues/020)) ([501c55b](https://github.com/eric8810/catcher/commit/501c55b715d53b623f100e2beb8563740c61c8b6))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* resolve 9 documented issues ([#003](https://github.com/eric8810/catcher/issues/003) [#006](https://github.com/eric8810/catcher/issues/006) [#008](https://github.com/eric8810/catcher/issues/008) [#009](https://github.com/eric8810/catcher/issues/009) [#010](https://github.com/eric8810/catcher/issues/010) [#011](https://github.com/eric8810/catcher/issues/011) [#013](https://github.com/eric8810/catcher/issues/013) [#014](https://github.com/eric8810/catcher/issues/014) [#015](https://github.com/eric8810/catcher/issues/015) [#017](https://github.com/eric8810/catcher/issues/017) [#018](https://github.com/eric8810/catcher/issues/018)) ([609a840](https://github.com/eric8810/catcher/commit/609a8406fdad63d39c64384e55d3645adc3fc06b))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* UniFFI setup_scaffolding, add catcher_free_result, remove UDL ([1da6f30](https://github.com/eric8810/catcher/commit/1da6f303be47667c9ec943db6f8babedfa2158d1))
* **ws:** review fixes — reconnect message buffering, pong_timeout, close echo, native-tls removal, latency, docs ([0ef7b5e](https://github.com/eric8810/catcher/commit/0ef7b5eb46f67eff579adacd078a4147f8c0efcf))
* **ws:** share dns resolver and retry handshakes ([b16471a](https://github.com/eric8810/catcher/commit/b16471af60e57c66af5d32042478caabd19a04c2))
