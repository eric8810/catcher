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
