## 0.3.12

### Packaging

- Add `MinimumOSVersion` 15.0 to the bundled iOS `catcher_ffi.framework` Info.plist for App Store upload validation.
- Rebuild the bundled Apple native frameworks.

## [0.4.0](https://github.com/eric8810/catcher/compare/catcher_core-v0.3.13...catcher_core-v0.4.0) (2026-06-14)


### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* catcher-ffi umbrella crate + real FFI integration tests ([97a5b1e](https://github.com/eric8810/catcher/commit/97a5b1eebed3806486e62a3a8c52ed85a928a231))
* **catcher-ws:** add send_timeout_ms so half-open sends cannot stall the event loop ([88d36e0](https://github.com/eric8810/catcher/commit/88d36e019cd05a160f3defdd3d9215bc3f87eceb))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Dart FFI bindings + napi-ws types + CI update ([39ac501](https://github.com/eric8810/catcher/commit/39ac501c190a67b06ea311cd06af8ac9e1dabbc9))
* Enhance catcher_core with native FFI support and platform-specific builds ([07e83cc](https://github.com/eric8810/catcher/commit/07e83ccfac65994bf3f901e70fb7421b376ce341))


### Bug Fixes

* [#022](https://github.com/eric8810/catcher/issues/022) stream chunk base64, [#023](https://github.com/eric8810/catcher/issues/023) quality race, [#024](https://github.com/eric8810/catcher/issues/024) SSE block_on panic ([9ba65a8](https://github.com/eric8810/catcher/commit/9ba65a84bac67e5eb31bc8a4dca7fb349605cfa9))
* address PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([b51c10b](https://github.com/eric8810/catcher/commit/b51c10ba3e4dd5d7203e28e5f3bac9697bcf00f7))
* **bindings:** align old-addon guards and error messages ([0ad6380](https://github.com/eric8810/catcher/commit/0ad6380240da882061c640cf38b5be06d48e75c5))
* **catcher_core:** set iOS framework minimum OS version ([09601c0](https://github.com/eric8810/catcher/commit/09601c01afe3c688d716400a42bd30fd86b2351d))
* **catcher_core:** set iOS framework minimum OS version ([abb6713](https://github.com/eric8810/catcher/commit/abb6713523916dfb8d72134b4c19303ba51f9155))
* critical review issues — use-after-free, async UniFFI, timeout race ([03753f0](https://github.com/eric8810/catcher/commit/03753f0a33f5ef38d2235a1c4cbfd72a0da39ce3))
* **dart:** fix Dart FFI compilation errors ([78b9c4a](https://github.com/eric8810/catcher/commit/78b9c4af13529c3146b4050c6256e0a998957e8e))
* **dart:** sync Flutter/Dart with actual API surface ([ede4129](https://github.com/eric8810/catcher/commit/ede4129adb00d77755e43353046773575f890314))
* **dart:** wire dns and msgpack config ([d419a36](https://github.com/eric8810/catcher/commit/d419a36fb1f350b6e92d6b1e2c04eaca0ac60496))
* FFI HttpError as response JSON + Dart body_base64/data_base64 compat ([04d03a3](https://github.com/eric8810/catcher/commit/04d03a37b4a4dd8667fd72b68ec796536bd7912b))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* UniFFI setup_scaffolding, add catcher_free_result, remove UDL ([1da6f30](https://github.com/eric8810/catcher/commit/1da6f303be47667c9ec943db6f8babedfa2158d1))
* wire Flutter FFI calls, implement UniFFI WsClient, fix test scripts, add CI/release infra ([670f915](https://github.com/eric8810/catcher/commit/670f915c042ea28928fd2e6b0d413d27cb75693d))

## 0.3.11

### New features

- Update the native WebSocket transport to use `yawc`, enabling native permessage-deflate (RFC 7692) support in the bundled Rust implementation.
- Add `application_compression` config to `WsClientConfig` with gzip and zstd support for application-layer compression fallback.

### Fixes

- Improve Android native build reliability by exporting NDK `CC_*` and `AR_*` variables for cross-compiled native dependencies.
- Buffer and replay messages sent during WebSocket reconnection instead of silently dropping them.
- Add fast pong timeout detection within a single heartbeat cycle.
- Echo Close frames on receipt before disconnecting (RFC 6455 §5.5.1).
- Report actual reconnect latency in `Connected` events instead of 0 ms.
- Remove `native-tls` feature; TLS is handled entirely by `yawc/rustls-ring`.

### Packaging

- Refresh bundled native Rust dependency versions for the 0.3.11 release.

## 0.3.10

### Packaging

- Bump the Flutter package to `0.3.10` to keep it aligned with the fresh npm and Rust release.
- Rebuild the native bundles through the full release workflow.

## 0.3.9

### New features

- Add native DNS cache controls to `DnsConfig`: `cacheSize`, `negativeTtlSecs`, `staleTtlSecs`, and `staleOnError`.
- Add `msgpack` to `HttpClientConfig` for native HTTP JSON ↔ MessagePack body conversion.
- Add `dns` and `msgpack` to `WsClientConfig` so Flutter WebSocket clients can use DNS cache settings and native MessagePack conversion.

### Fixes

- Fix DNS config not being passed through the Dart FFI layer to the native HTTP and WebSocket clients.
- Fix built-in MessagePack config not being passed through the Dart FFI layer.

### Packaging

- Keep bundled Android, iOS, macOS, Linux, and Windows native libraries below pub.dev package size limits.

## 0.3.8

- Publish `catcher_core` as a Flutter FFI plugin with platform native bundle metadata.
- Bundle prebuilt Android, iOS, macOS, Linux, and Windows native libraries during pub.dev release.
- Refresh README installation guidance for the current package version.

## 0.3.1

- Package `catcher_core` as a Flutter FFI plugin with platform native bundle metadata.
- Bundle prebuilt Android, iOS, macOS, Linux, and Windows native libraries during pub.dev release.
- Load Apple builds from `catcher_ffi.framework/catcher_ffi` and desktop/mobile dynamic libraries from app bundle search paths.

## 0.2.2

- SSE client: `CatcherSseClient` (persistent + auto-reconnect) and `sseStream()` (one-shot).
- Per-request headers + timeout in `get()`, `post()`, `sseStream()`.
- `cancelAll()`, `circuitBreakerState`, `metrics`, `setAdaptiveTimeout()`.
- Full config passthrough: `TlsConfig`, `DnsConfig`, `ProxyConfig`, `RedirectConfig`.
- `WsClientConfig` now includes `headers`, `protocols`, `deflateThresholdBytes`, `raceCount`.
- `qualityHistory()` for persistent sliding window network quality data.

## 0.1.0

- Initial release.
- HTTP client with retries, timeouts, keep-alive.
- WebSocket client with reconnection and permessage-deflate.
- FFI bindings to Rust core.
