# Changelog

All notable changes to this project will be documented in this file. See [release-please](https://github.com/googleapis/release-please) for automated management.

## 0.3.2 (2026-07-20)

### 🐛 Bug Fixes

- **napi-http TS types (GitHub #3)**: Hand-written TypeScript types used `snake_case` but NAPI-RS auto-generates `camelCase`. Replaced hand-written types with re-exports from auto-generated `index.d.ts`. JSON config types (serde-based) correctly retain `snake_case` with `camelCase` alias support.
- **napi-ws TLS missing (GitHub #4)**: `catcher-ws` did not enable any TLS feature for `tokio-tungstenite`, causing all `wss://` connections to fail immediately. Added `rustls-tls` (webpki-roots) as default feature, consistent with `catcher-http`.
- **`catcher_core/rust` workspace isolation**: Added empty `[workspace]` table to `catcher_core/rust/Cargo.toml` to prevent it from incorrectly inheriting the monorepo workspace.
- **`normalizeOptions` cleanup**: Rewritten to build clean option objects instead of spreading raw input (which leaked stale snake_case properties).

## 0.3.1 (2026-05-18)

### 🐛 Bug Fixes

- Republish all packages from correct commit (v0.3.0 napi/crates.io packages were from stale tag).
- Clippy: ~40 fixes including redundant closures, auto-deref, saturating_sub, div_ceil, slice::from_ref, etc.
- FFI: `catcher_free_event_data` made `pub unsafe extern "C"` for correct visibility.
- FFI: Added safety documentation comments to all unsafe functions.
- `catcher_core/rust/Cargo.toml` version sync (was stuck at 0.2.2).
- Test files: wrapped unsafe FFI calls in `unsafe {}` blocks.

## 0.3.0 (2026-07)

### ⚠️ Breaking Changes

- **napi packages**: Entry point changed from `client.js` → `dist/client.js`. Config now accepts typed objects (not just JSON strings). Class names renamed: `JsHttpClient` → `HttpClient`, `JsWsClient` → `WsClient`. Callback events are now typed objects (auto-parsed) instead of JSON strings. WS message data uses `event.data_base64` (base64).
- **Rust crates**: `BackoffKind::default()` changed from `Exponential` → `Fixed`. WS config fields renamed: `deflate_threshold` → `deflate_threshold_bytes`, `max_message_size` → `max_payload_bytes`, `ping_timeout_ms` → `pong_timeout_ms`. All config structs now support `snake_case` + `camelCase` via `#[serde(alias)]`.
- **reqwest 0.13 + tungstenite 0.29**: Dependency upgrade; may affect custom TLS configurations.

### 🚀 New Features

- **Typed napi TS wrappers**: Auto-generated TypeScript sources replace hand-written wrappers. Full type safety for config, events, and responses.
- **Certificate pinning** (`pin_sha256`): Rust-side TLS certificate public key pinning for HTTP clients.
- **Multipart/form-data encoder** (Rust): Native multipart upload support in `catcher-http`.
- **DNS nameservers config**: Custom DNS resolver addresses for Rust HTTP client.
- **Web progress events**: Browser package (`catcher-web`) progress tracking for downloads/uploads.
- **ESM export fix**: All TS packages now correctly export ESM entry points.
- **Dart FFI config alignment**: Dart `HttpClientConfig` / `WsClientConfig` fields now match Rust config 1:1.
- **`#[serde(alias)]` on all configs**: Every config struct accepts both `snake_case` and `camelCase` JSON keys.

### 🐛 Bug Fixes

- Fixed 9 documented issues (#001–#006, #008–#010): memory leak in multipart, SSE O(n²) buffer, P90 repeated sort, config clone per-request, handle registry lock contention, circuit breaker TOCTOU, WS heartbeat RTT always zero, SSE reconnect recreates client, stream chunk copy.
- Fixed UniFFI issues (#011–#018): stream chunk to vec copy, priority queue single worker, uniffi block-on-aux-thread, catcher-free-data UB, evaluate-quality race panic, SSE stream buffer-all, FFI stream cancel, WS client drop no close.
- Fixed FFI body base64 encoding (#019, #021) and adaptive heartbeat timer (#020).
- Fixed UniFFI evaluate_quality take/put race (#023), stream chunk base64 (#022), SSE block_on panic (#024).
- Fixed `http_retries` metric wiring via custom `MetricsRetryMiddleware`.
- Fixed proxy bandwidth variable name bug.

### 🔄 Dependencies

- Upgraded `reqwest` 0.12 → 0.13
- Upgraded `tungstenite` 0.26 → 0.29, `tokio-tungstenite` 0.24 → 0.29

### 📝 Documentation

- Added breaking change notices to all package READMEs.
- Added `docs/arch-rs/17-dart-config-alignment.md` — Dart FFI config alignment design.
- Updated `docs/arch-rs/01-cargo.md` for v0.3 workspace.
- Updated all version references across docs.
- Added expansion research reports with citations.

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
cargo clippy --workspace --all-targets -- -D warnings  # 0 warnings
cargo test --workspace                   # 88/90 passed (2 network-dependent)
pnpm test                                # 323 passed, 2 skipped
pnpm test:e2e                            # TS + Rust E2E scenarios
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
