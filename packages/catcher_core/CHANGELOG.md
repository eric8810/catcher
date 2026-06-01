## 0.3.11

### New features

- Update the native WebSocket transport to use `yawc`, enabling native permessage-deflate support in the bundled Rust implementation.

### Fixes

- Improve Android native build reliability by exporting NDK `CC_*` and `AR_*` variables for cross-compiled native dependencies.

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
