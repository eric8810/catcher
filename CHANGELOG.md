# Changelog

All notable changes to this project will be documented in this file. See [release-please](https://github.com/googleapis/release-please) for automated management.

## 0.2.2 (2026-06)

### 🚀 New Features — FFI

- **SSE C ABI** (FFI-02): 6 new `#[no_mangle]` symbols — `catcher_sse_connect`, `catcher_sse_stream`, `catcher_sse_ready_state`, `catcher_sse_last_event_id`, `catcher_sse_close`, `catcher_sse_destroy`. Dart `CatcherSseClient` + `sseStream()` bindings included.
- **Per-request headers + timeout** (FFI-01): `catcher_http_execute`, `catcher_http_get`, `catcher_http_post` now accept `headers_json` and `timeout_ms` parameters.
- **Circuit breaker state query** (FFI-05): `catcher_http_circuit_breaker_state` C ABI symbol. Dart `circuitBreakerState` getter.
- **Runtime metrics** (FFI-07): `catcher_http_metrics` returns `MetricsSnapshot` JSON. Dart `metrics` getter.
- **Cancel all in-flight requests** (FFI-04): `catcher_http_client_cancel_all`. Uses `Arc<Mutex<CancellationToken>>` with replacement semantics.
- **Adaptive timeout** (FFI-10): `catcher_http_adaptive_timeout_config` — P90 RTT-based auto-tuning.
- **Network quality history** (FFI-09): `catcher_quality_history` — persistent sliding window query.
- **TLS/DNS/Proxy config passthrough** (FFI-06): Full `TlsConfig`, `DnsConfig`, `ProxyConfig`, `RedirectConfig`, `Auth`, `default_headers` now passed via `HttpClientConfig` JSON from Dart.
- **WS config passthrough** (FFI-03/FFI-12): Dart `WsClientConfig` now includes `headers`, `protocols`, `deflateThresholdBytes`, `raceCount`.

### 🧪 Testing

- **14 new Rust FFI integration tests** (TEST-01): `catcher-ffi/tests/{http_test.rs, sse_test.rs, codec_quality_test.rs}` using wiremock mock servers.
- **19 new CatcherError unit tests** (TEST-08): Full error category + retryable classification coverage in `catcher-core/src/error.rs`.

### 📐 C ABI Symbol Count: 16 → 25

| Module | Count | Symbols |
|--------|:-----:|---------|
| HTTP | 9 | client_create, client_destroy, execute, get, post, client_cancel_all, circuit_breaker_state, metrics, adaptive_timeout_config |
| SSE | 6 | connect, stream, ready_state, last_event_id, close, destroy |
| WS | 5 | create, send_text, send_binary, close, destroy |
| Codec | 3 | pack, unpack, free_data |
| Quality | 2 | evaluate_quality, quality_history |

### 📝 Documentation

- `docs/arch-rs/09-ffi.md` — Updated from 16 to 25 symbols, fixed file paths, signatures, and comparison tables.
- `docs/issues/ffi-uniffi-capability-gaps.md` — Marked FFI-01~07, 09~10, 12 as ✅; updated test coverage; refreshed roadmap.
- `docs/plan/10-ffi-capability-gap-design.md` — Added implementation status, updated test section, roadmap.
- `docs/plan/handoff.md` — Updated symbol counts and completion status.

### ✅ Verification

```
cargo check --workspace --all-targets    # 0 errors, 0 warnings
cargo test --workspace                   # 142/142 passed
pnpm test                                # 323 passed, 2 skipped
pnpm test:e2e                            # 38/38 passed
pnpm bench                               # 5 benchmark groups
```

## 0.2.1

- Initial public release.
- HTTP client with retries, circuit breaker, keep-alive, priority queue.
- WebSocket client with reconnection, multi-endpoint racing, perMessageDeflate.
- SSE client (TS) with auto-reconnect and one-shot stream.
- FFI bindings to Rust core (16 C ABI symbols).
- napi-rs bindings (Node.js HTTP + WS native addons).
- Dart FFI bindings (catcher_core pub.dev package).
- UniFFI bindings (Swift + Kotlin skeleton).

## 0.1.0

- Initial release.
- HTTP client with retries, timeouts, keep-alive.
- WebSocket client with reconnection and permessage-deflate.
- FFI bindings to Rust core.
