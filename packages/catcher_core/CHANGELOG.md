## 0.4.0

### Packaging

- Add Swift Package Manager support for the Flutter iOS plugin: `ios/catcher_core/Package.swift` exposes `catcher_ffi.xcframework` as a binary target, so Flutter apps can link `catcher_core` without CocoaPods.
- Build scripts and the pub.dev release workflow now also assemble `ios/catcher_core/catcher_ffi.xcframework` for SPM consumers.

## [0.6.0](https://github.com/eric8810/catcher/compare/catcher_core-v0.5.0...catcher_core-v0.6.0) (2026-09-21)


### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* catcher-ffi umbrella crate + real FFI integration tests ([97a5b1e](https://github.com/eric8810/catcher/commit/97a5b1eebed3806486e62a3a8c52ed85a928a231))
* **catcher-ws:** add send_timeout_ms so half-open sends cannot stall the event loop ([88d36e0](https://github.com/eric8810/catcher/commit/88d36e019cd05a160f3defdd3d9215bc3f87eceb))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Dart FFI bindings + napi-ws types + CI update ([39ac501](https://github.com/eric8810/catcher/commit/39ac501c190a67b06ea311cd06af8ac9e1dabbc9))
* Enhance catcher_core with native FFI support and platform-specific builds ([07e83cc](https://github.com/eric8810/catcher/commit/07e83ccfac65994bf3f901e70fb7421b376ce341))
* update iOS deployment target to 15.0 and adjust framework generation ([d06bbe9](https://github.com/eric8810/catcher/commit/d06bbe9360a2c362c6cc44d43a6802cce5155b61))


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
* **flutter:** restore WebSocket connection and prevent startup crash ([#21](https://github.com/eric8810/catcher/issues/21)) ([9882a08](https://github.com/eric8810/catcher/commit/9882a084dec0361ad7c49dfc564aeddb144d2b03))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* **release:** align internal Rust dependency versions ([5e8a199](https://github.com/eric8810/catcher/commit/5e8a199e27e3556cbf874963350a94238a3563ad))
* **release:** align internal Rust dependency versions ([254580c](https://github.com/eric8810/catcher/commit/254580cfccaa9df8ad65d7d20c710ccfdcfa7441))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* UniFFI setup_scaffolding, add catcher_free_result, remove UDL ([1da6f30](https://github.com/eric8810/catcher/commit/1da6f303be47667c9ec943db6f8babedfa2158d1))
* wire Flutter FFI calls, implement UniFFI WsClient, fix test scripts, add CI/release infra ([670f915](https://github.com/eric8810/catcher/commit/670f915c042ea28928fd2e6b0d413d27cb75693d))
* **ws:** restore Flutter Android socket connectivity ([7787c61](https://github.com/eric8810/catcher/commit/7787c61fa0b4427c9031a11b61691ee1145f4588))
* **ws:** restore Flutter WebSocket direct connections ([8455329](https://github.com/eric8810/catcher/commit/845532995f376fac41dcc94158c791f47201af57))

## [0.5.0](https://github.com/eric8810/catcher/compare/catcher_core-v0.4.0...catcher_core-v0.5.0) (2026-09-21)


### Features

* add application-layer compression support with gzip and zstd ([10f43f4](https://github.com/eric8810/catcher/commit/10f43f4c54399bb15809cde141b5db85bc4ab37a))
* **bindings:** expose networkChanged() in napi, uniffi and dart bindings ([ebdf7d7](https://github.com/eric8810/catcher/commit/ebdf7d7a3c79798e32d6b6ae5991fd5b3bc4b93b))
* catcher-ffi umbrella crate + real FFI integration tests ([97a5b1e](https://github.com/eric8810/catcher/commit/97a5b1eebed3806486e62a3a8c52ed85a928a231))
* **catcher-ws:** add send_timeout_ms so half-open sends cannot stall the event loop ([88d36e0](https://github.com/eric8810/catcher/commit/88d36e019cd05a160f3defdd3d9215bc3f87eceb))
* complete platform coverage — napi-http/ws full API, @catcher/web, Flutter dart:ffi, UniFFI, docs sync ([3c5197f](https://github.com/eric8810/catcher/commit/3c5197ff51a1d738f2c76d160fbd979490c2af82))
* Dart FFI bindings + napi-ws types + CI update ([39ac501](https://github.com/eric8810/catcher/commit/39ac501c190a67b06ea311cd06af8ac9e1dabbc9))
* Enhance catcher_core with native FFI support and platform-specific builds ([07e83cc](https://github.com/eric8810/catcher/commit/07e83ccfac65994bf3f901e70fb7421b376ce341))
* update iOS deployment target to 15.0 and adjust framework generation ([d06bbe9](https://github.com/eric8810/catcher/commit/d06bbe9360a2c362c6cc44d43a6802cce5155b61))


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
* **flutter:** restore WebSocket connection and prevent startup crash ([#21](https://github.com/eric8810/catcher/issues/21)) ([9882a08](https://github.com/eric8810/catcher/commit/9882a084dec0361ad7c49dfc564aeddb144d2b03))
* PR [#13](https://github.com/eric8810/catcher/issues/13)–15 review findings + release 0.3.13 (issues [#28](https://github.com/eric8810/catcher/issues/28)–34) ([0320595](https://github.com/eric8810/catcher/commit/032059537d829d8535df7b0326c98c7c33ccc1a7))
* review round 2 — 36 issues across Rust/Dart/infra ([fe0893e](https://github.com/eric8810/catcher/commit/fe0893e7b5d96d7f5be879ebe2acdc5602b531b2))
* review round 3 — 11 issues across Rust/Dart/infra ([7e62159](https://github.com/eric8810/catcher/commit/7e6215981ee654a94426853d26fa3df6be90211f))
* support explicit proxy for mobile clients ([a45368d](https://github.com/eric8810/catcher/commit/a45368d99f5045ececa13e0e18430919b53c38d1))
* support proxy dns behavior across http and ws ([d8ff3df](https://github.com/eric8810/catcher/commit/d8ff3df955a42fb37371e8c4714125d2af845897))
* support proxy DNS behavior across HTTP and WS ([15b1233](https://github.com/eric8810/catcher/commit/15b12333e5a233fef4ed1419c498f85f2dfb2af2))
* UniFFI setup_scaffolding, add catcher_free_result, remove UDL ([1da6f30](https://github.com/eric8810/catcher/commit/1da6f303be47667c9ec943db6f8babedfa2158d1))
* wire Flutter FFI calls, implement UniFFI WsClient, fix test scripts, add CI/release infra ([670f915](https://github.com/eric8810/catcher/commit/670f915c042ea28928fd2e6b0d413d27cb75693d))
* **ws:** restore Flutter Android socket connectivity ([7787c61](https://github.com/eric8810/catcher/commit/7787c61fa0b4427c9031a11b61691ee1145f4588))
* **ws:** restore Flutter WebSocket direct connections ([8455329](https://github.com/eric8810/catcher/commit/845532995f376fac41dcc94158c791f47201af57))

## 0.3.19

### Bug Fixes

- Preserve structured transport failure details through NAPI HTTP retries, including error codes and chained causes, so host applications can diagnose connection failures after retries are exhausted.

### Packaging

- Synchronize the Flutter package version with the unified Catcher `0.3.19` release; the Dart API surface is otherwise unchanged.

## 0.3.18

### Bug Fixes

- **HTTP 421 recovery**: native HTTP requests rebuild the current transport's connection pool and retry once after a `421 Misdirected Request`, without cancelling unrelated in-flight requests.
- **Structured NAPI HTTP errors**: the Node.js NAPI wrapper exports `HttpError` with `status`, `body`, and `cause` fields so host applications can inspect HTTP status failures directly.

### Packaging

- Synchronize all Catcher package versions to `0.3.18`; the Flutter API surface is otherwise unchanged.

## 0.3.17

### Bug Fixes

- **Flutter WebSocket direct connections**: default simple WebSocket connections now use the yawc native backend, restoring Android Flutter socket connectivity while keeping reqwest for proxy, Catcher DNS, and advanced TLS configurations.
- **WebSocket network change recovery**: `networkChanged()` immediately reconnects, resets reconnect backoff, re-races all endpoints for multi-endpoint configs, and replays buffered send commands after reconnect.
- **WebSocket old native library compatibility**: missing `catcher_ws_network_changed` is handled lazily so older native libraries can still construct clients; calling `networkChanged()` reports a clear rebuild requirement.
- **Apple framework metadata**: generated iOS/macOS `catcher_ffi.xcframework` bundles include minimum OS metadata, and podspec versions are synchronized for this release.

### Behavior Changes

- **Rust `EndpointRacer` internalized**: multi-endpoint racing is now an implementation detail of `WsTransport::connect()` and is no longer exposed as `catcher_ws::EndpointRacer`. Configure `WsClientConfig.urls` / `race_count` instead.

## 0.3.16

### Bug Fixes

- **napi `system-proxy` feature not enabled**: 0.3.15 introduced system proxy
  auto-detection (`proxy.mode = "system"`), but both napi bindings
  (`catcher-napi-http`, `catcher-napi-ws`) did not enable the `system-proxy` cargo
  feature on their dependency, so `detect_system_proxy()` compiled to a no-op stub
  and `proxy.mode = "system"` silently did nothing in the published npm packages.
  The feature is now enabled on both napi crates.
- **napi TypeScript types synced**: `ProxyConfig` is now modeled as a discriminated
  union in the published TS types — `ManualProxyConfig` (default) requires `url`,
  `SystemProxyConfig` (`mode: 'system'`) may omit it. This tightens the types so
  url-less manual configs (which would panic in `transport_url()`) no longer
  type-check. The Dart package is unaffected (no napi binding).

## 0.3.15

### Features

- **`ProxyMode` enum** (`Manual` / `System`): caller can set `proxy.mode = "system"` to
  let catcher auto-detect the system proxy from the OS instead of passing a URL manually.
- **System proxy auto-detection** via `proxy-cfg`: macOS (SystemConfiguration), Windows
  (WinINET registry + WinHTTP), Linux (env vars + /etc/sysconfig/proxy).
- **`networkChanged()` re-detects system proxy**: when the network environment changes,
  system proxy is re-read and the reqwest client is rebuilt if the proxy address changed.

### ⚠️

- **`ProxyConfig.url`: `String` → `Option<String>`**: a pre-1.0 breaking change. JSON
  deserialization is backward-compatible (`#[serde(default)]` ensures `mode` defaults
  to `Manual` when absent).

## 0.3.13

### Behavior Changes

- DNS is now opt-in: without a `dns` config the platform's native resolver is used
  (previously the Catcher resolver was always built). Pass `dns` to re-enable
  caching / host mapping / custom nameservers.
- `socks5://` proxies are normalized to `socks5h://` so the target domain is
  resolved remotely at the proxy (fixes Clash / VPN domain routing).
- `tls_sni_override` is rejected on the native transport (was silently ignored).

### Bug Fixes

- FFI lifecycle: destroying a client now stops in-flight requests/SSE and no longer
  invokes the host callback afterwards (prevents use-after-free).

## 0.3.12

### Packaging

- Add `MinimumOSVersion` 15.0 to the bundled iOS `catcher_ffi.framework` Info.plist for App Store upload validation.
- Rebuild the bundled Apple native frameworks.

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
