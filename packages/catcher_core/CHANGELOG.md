## 0.3.13

### Features

- `networkChanged()` on HTTP and WebSocket clients for proactive recovery after
  network switches (WiFi ↔ cellular, VPN connect/disconnect): clears DNS cache,
  rebuilds the connection pool, resets the circuit breaker, and reconnects WS.
- WebSocket now supports explicit `proxy` and `tls` config; HTTP and WS share the
  same `ProxyConfig` / `TlsConfig`, with SOCKS5/SOCKS5h and HTTP CONNECT support.

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
