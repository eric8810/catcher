# 原生能力缺口 — 测试设计

> 对应 Issues：[native-layer-capability-gaps.md](./native-layer-capability-gaps.md) N-01 ~ N-04
> 测试框架：Rust `#[tokio::test]` + `wiremock` | FFI C ABI `cargo test -p catcher-ffi` | Dart `dart test`
> 日期：2026-06

---

## 测试策略总览

```
N-01 Multipart  ──── Rust unit (builder) + FFI integ (5 C symbols) + Dart unit (encoder)
N-02 Stream      ──── Rust unit (execute_stream) + FFI integ (streaming callback)
N-03 Cancel      ──── Rust unit (per-request token map) + FFI integ (cancel by id)
N-04 Quality     ──── Rust unit (subscription lifecycle) + FFI integ (push callback)
```

每个缺口覆盖 **3 层**：Rust 核心逻辑 → FFI C ABI 边界 → Dart 绑定层。

---

## 测试文件规划

| 测试文件 | 覆盖 | 位置 | 预计用例数 |
|---------|------|------|:--------:|
| `http_client.rs` (扩展现有 `#[cfg(test)]`) | N-02 execute_stream, N-03 per-request cancel | `packages/catcher-http/src/transport/` | +15 |
| `network_quality.rs` (扩展现有 `#[cfg(test)]`) | N-04 QualitySubscription | `packages/catcher-http/src/observability/` | +8 |
| `multipart/builder.rs` (新增模块) | N-01 MultipartBuilder (方案B) | `packages/catcher-http/src/multipart/` | +10 |
| `http_test.rs` (扩展现有 FFI test) | N-02 FFI stream, N-03 FFI cancel | `packages/catcher-ffi/tests/` | +10 |
| `quality_test.rs` (拆分自 codec_quality_test) | N-04 FFI subscribe/unsubscribe | `packages/catcher-ffi/tests/` | +6 |
| `multipart_test.rs` (新增 FFI test) | N-01 FFI multipart C symbols | `packages/catcher-ffi/tests/` | +8 |
| `http_client_test.dart` (扩展) | N-03 Dart cancel, N-02 Dart stream | `packages/catcher_core/test/` | +8 |
| `quality_test.dart` (扩展) | N-04 Dart quality subscribe | `packages/catcher_core/test/` | +4 |

---

## N-02: 流式下载测试

### Rust 核心层 — `http_client.rs` #[cfg(test)]

```rust
// ── execute_stream 基础流程 ──
// 注：wiremock 无法模拟 TCP 级别的 chunk 分割（body 一次性返回）。
// 以下测试聚焦回调事件类型和顺序。逐 chunk 行为由 `bytes_stream()` 保证，
// 属于 reqwest 底层行为，无需单独验证。

#[tokio::test]
async fn ns02_stream_receives_body_via_callback() {
    // wiremock 返回 200 + body → 验证 callback 收到 Headers + 至少 1 个 Chunk + Done
    // chunk 数据拼接后 == 原始 body
}

#[tokio::test]
async fn ns02_stream_headers_event_fires_first() {
    // 验证第一个 event 是 Headers，包含 status=200
}

#[tokio::test]
async fn ns02_stream_done_event_fires_last() {
    // 验证最后一个 event 是 Done
}

#[tokio::test]
async fn ns02_stream_empty_body() {
    // 验证空响应体：只有 Headers + Done，无 Chunk
}

#[tokio::test]
async fn ns02_stream_error_on_4xx() {
    // wiremock 返回 404 → Error event
}

#[tokio::test]
async fn ns02_stream_error_on_5xx() {
    // wiremock 返回 500 → Error event（服务端错误也应流式报告）
}

#[tokio::test]
async fn ns02_stream_cancel_mid_stream() {
    // 发送中调用 cancel_all() → 流中断 → 后续 chunk 不再回调
}

#[tokio::test]
#[ignore = "requires platform RSS monitoring, manual E2E only"]
async fn ns02_stream_memory_bounded() {
    // 100MB 响应 → 验证 process RSS 不随 body size 线性增长
    // 不适合 CI 自动化，标记 #[ignore]
}
```

### FFI C ABI 层 — `http_test.rs` 扩展

```rust
// ── catcher_http_execute_stream ──

#[tokio::test]
async fn h10_execute_stream_chunks() {
    // wiremock POST /stream → 200 + body
    // 调用 catcher_http_execute_stream → 回调 capture chunks
    // 验证 chunks 数量 > 0，数据拼接完整
}

#[tokio::test]
async fn h11_execute_stream_headers_event() {
    // 验证第一个 callback event_type == "headers"
    // 验证 event_data JSON 包含 status 和 content-length
}

#[tokio::test]
async fn h12_execute_stream_chunk_event() {
    // 验证中间 callback event_type == "chunk"
    // event_data 是原始字节
}

#[tokio::test]
async fn h13_execute_stream_done_event() {
    // 验证最后一个 callback event_type == "done"
    // event_data JSON 包含 elapsed_ms
}

#[tokio::test]
async fn h14_execute_stream_error_4xx() {
    // wiremock 返回 404 → callback event_type == "error"
}

#[tokio::test]
async fn h15_execute_stream_cancel() {
    // 发送流式请求 → 立即 cancel_all → callback event_type == "error"
    // event_data 包含 {"message":"request cancelled"}
}

#[tokio::test]
async fn h16_execute_stream_large_body() {
    // 50MB 响应 → 验证不 OOM (可选，标记 #[ignore] 在 CI 中跳过)
}
```

### Dart 绑定层

```dart
// packages/catcher_core/test/http_client_test.dart

test('NS02-D01: executeStream yields HttpStreamEvent chunks', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  final events = <HttpStreamEvent>[];
  final stream = client.executeStream('GET', '/stream');
  await for (final event in stream) {
    events.add(event);
  }
  expect(events.first.type, equals(HttpStreamEventType.headers));
  expect(events.last.type, equals(HttpStreamEventType.done));
  expect(events.where((e) => e.type == HttpStreamEventType.chunk).length, greaterThan(0));
});

test('NS02-D02: executeStream cancel mid-stream', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  final stream = client.executeStream('GET', '/slow-stream');
  final subscription = stream.listen((_) {});
  await Future.delayed(Duration(milliseconds: 100));
  await subscription.cancel();
  // subscription 正常关闭，无泄漏
});
```

---

## N-03: Per-request Cancel 测试

### Rust 核心层 — `http_client.rs` #[cfg(test)]

```rust
// ── Per-request cancel ──

#[tokio::test]
async fn ns03_execute_returns_unique_ids() {
    // 连续 3 次 execute() → 3 个互不相同、单调递增的 request_id
}

#[tokio::test]
async fn ns03_cancel_request_only_cancels_target() {
    // 发起 req_1 (slow), req_2 (slow), req_3 (fast)
    // cancel_request(req_1_id) → req_1 返回 Cancelled
    // req_2 和 req_3 继续正常完成
}

#[tokio::test]
async fn ns03_cancel_request_nonexistent_returns_false() {
    // cancel_request(99999) → false
}

#[tokio::test]
async fn ns03_cancel_all_cancels_everything() {
    // 发起 req_1, req_2, req_3 (全部 slow)
    // cancel_all() → 全部返回 Cancelled
}

#[tokio::test]
async fn ns03_cancel_all_then_new_requests_work() {
    // cancel_all() → 新 execute() → 正常完成（不受前次 cancel 影响）
    // 新请求的 request_id 不受旧 id 影响
}

#[tokio::test]
async fn ns03_concurrent_requests_independent() {
    // 同时飞行 10 个请求，cancel_request 其中 3 个
    // 验证其余 7 个正常完成，被 cancel 的 3 个返回 Cancelled
}

#[tokio::test]
async fn ns03_cancel_idempotent() {
    // cancel_request(id) 调用两次 → 第一次 true，第二次 false（已移除）
}
```

### FFI C ABI 层 — `http_test.rs` 扩展

```rust
// ── catcher_http_execute 返回值 + catcher_http_cancel_request ──

#[tokio::test]
async fn h17_execute_returns_request_id() {
    // catcher_http_execute → 返回值 > 0 (有效 request_id)
}

#[tokio::test]
async fn h18_cancel_request_by_id() {
    // execute 发起 slow 请求 → cancel_request(id) 返回 0 (成功)
    // callback 收到 event_type="error", event_data 包含 "cancelled"
}

#[tokio::test]
async fn h18b_execute_callback_receives_response() {
    // execute 正常请求 → callback 收到 event_type="response"
    // event_data JSON 包含 status, body, headers, elapsed_ms
    // request_id 在 callback JSON 中对应
}

#[tokio::test]
async fn h19_cancel_request_nonexistent() {
    // cancel_request(99999) → 返回 -1
}

#[tokio::test]
async fn h20_cancel_all_still_works() {
    // execute 多个请求 → cancel_all() → 所有 callback 收到 cancelled
    // 新请求可正常执行
}

#[tokio::test]
async fn h21_execute_returns_zero_on_null_handle() {
    // catcher_http_execute(NULL, ...) → 返回 0
}

#[tokio::test]
async fn h22_cancel_request_null_handle() {
    // catcher_http_cancel_request(NULL, 1) → 返回 -1
}
```

### Dart 绑定层

> Dart API 设计：所有请求方法返回 `RequestHandle { int requestId; Future<HttpResponse> response; }`。
> 仅需 cancel 的单请求通过 `requestHandle.requestId` 获取 id 后调用 `client.cancelRequest(id)`。
> 不关心 cancel 的代码直接 `await requestHandle.response` 即可。

```dart
// packages/catcher_core/test/http_client_test.dart

test('NS03-D01: get returns RequestHandle with valid requestId', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  final handle = client.get('/echo');
  expect(handle.requestId, greaterThan(0));
  final resp = await handle.response;
  expect(resp.status, equals(200));
});

test('NS03-D02: cancelRequest cancels only target', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  final h1 = client.get('/delay?ms=2000');
  final h2 = client.get('/delay?ms=2000');
  final h3 = client.get('/echo'); // fast

  final result = client.cancelRequest(h1.requestId);
  expect(result, isTrue);

  // h1.response 应该 throw CatcherError(type: cancelled)
  await expectLater(h1.response, throwsA(predicate((e) => e.type == 'cancelled')));
  // h2, h3 应该正常完成
  final resp3 = await h3.response;
  expect(resp3.status, equals(200));
  final resp2 = await h2.response;
  expect(resp2.status, equals(200));
});

test('NS03-D03: cancelAll cancels all in-flight', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  final h1 = client.get('/delay?ms=2000');
  final h2 = client.get('/delay?ms=2000');
  client.cancelAll();

  // h1, h2 都应 throw cancelled
  await expectLater(h1.response, throwsA(predicate((e) => e.type == 'cancelled')));
  await expectLater(h2.response, throwsA(predicate((e) => e.type == 'cancelled')));

  // 新请求不受影响
  final h3 = client.get('/echo');
  final resp3 = await h3.response;
  expect(resp3.status, equals(200));
});

test('NS03-D04: cancelRequest nonexistent returns false', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  expect(client.cancelRequest(99999), isFalse);
});

test('NS03-D05: back compat — await requestHandle.response works', () async {
  final client = CatcherHttpClient(baseUrl: serverUri);
  // 不关心 requestId 的代码直接用 .response
  final resp = await client.get('/echo').response;
  expect(resp.status, equals(200));
});
```

---

## N-04: Quality Push Events 测试

### Rust 核心层 — `network_quality.rs` #[cfg(test)]

```rust
// ── QualitySubscription ──

#[tokio::test]
async fn ns04_subscription_starts_measurement() {
    // 创建 QualitySubscription → 首次回调在 interval_ms 内触发
    // 验证回调被调用，携带 level、avg_rtt_ms 等字段
}

#[tokio::test]
async fn ns04_unsubscribe_stops_callbacks() {
    // subscribe → 等 2 次回调 → unsubscribe → 等 3 × interval_ms
    // 验证 unsubscribe 后回调不再触发
}

#[tokio::test]
async fn ns04_level_change_triggers_callback() {
    // 初始 Excellent → 手动插入高 RTT → evaluate → level 变 Poor
    // 验证回调触发，previous_level = "excellent", trend = "degrading"
}

#[tokio::test]
async fn ns04_no_callback_on_same_level() {
    // 连续两次 measure 返回相同 level → 第二次不触发回调
}

#[tokio::test]
async fn ns04_trend_computation() {
    // Excellent → Good: "degrading"
    // Poor → Fair: "improving"
    // Good → Good: "stable"
    // None → Excellent: "unknown"
}

#[tokio::test]
async fn ns04_multiple_subscribers_independent() {
    // 创建 2 个订阅者 → 各自独立接收回调
    // unsubscribe 订阅者 1 → 订阅者 2 继续接收
}

#[tokio::test]
async fn ns04_subscription_drop_cleans_up() {
    // subscribe → drop handle → 后台 task 退出（无泄漏）
    // 验证 tokio task 数量减少
}

#[tokio::test]
async fn ns04_measurement_failure_no_crash() {
    // host 不可达 → measure_http_rtt 失败 → 不影响下次 interval
    // 验证 task 不 panic，继续运行
}

#[tokio::test]
async fn ns04_timing_respects_interval() {
    // 设置 interval=200ms → 验证连续 2 次回调间隔 ≈ 200ms (±50ms 容差)
    // 使用 tokio::time::Instant 记录回调时间戳
}
```

### FFI C ABI 层 — `quality_test.rs`（新增独立文件）

```rust
// packages/catcher-ffi/tests/quality_test.rs

#[tokio::test]
async fn q02_subscribe_receives_callback() {
    // catcher_quality_subscribe(host, 500, callback, user_data)
    // 等 1s → 验证 callback 被调用 ≥ 1 次
    // callback JSON 包含 level, avg_rtt_ms, jitter_ms, sample_count
}

#[tokio::test]
async fn q03_subscribe_unsubscribe() {
    // subscribe → 等一次回调 → unsubscribe
    // 等 3 × interval_ms → 验证回调不再增加
}

#[tokio::test]
async fn q04_subscribe_invalid_host() {
    // subscribe("http://127.0.0.1:1", 500, callback)
    // 验证 callback 收到包含 error 字段的 JSON
    // task 不 panic
}

#[tokio::test]
async fn q05_subscribe_multiple() {
    // 创建 2 个订阅 → 各自独立 → unsubscribe 第一个
    // 第二个继续收到回调
}

#[tokio::test]
async fn q06_subscribe_null_callback() {
    // catcher_quality_subscribe(host, 500, NULL, user_data)
    // 返回 NULL 或有效 handle（取决于实现策略）
}

#[tokio::test]
async fn q07_unsubscribe_null_handle() {
    // catcher_quality_unsubscribe(NULL) → 不 crash
}
```

### Dart 绑定层

```dart
// packages/catcher_core/test/quality_test.dart

test('NS04-D01: subscribe receives quality events', () async {
  final events = <NetworkQualityEvent>[];
  final sub = CatcherQuality.subscribe(
    serverUri,
    intervalMs: 500,
    onEvent: (event) => events.add(event),
  );
  await Future.delayed(Duration(seconds: 3));
  await sub.unsubscribe();  // async — signals Rust to stop
  expect(events.length, greaterThan(0));
  expect(events.first.level, isNotNull);
  expect(events.first.avgRttMs, greaterThan(0));
});

test('NS04-D02: unsubscribe stops events', () async {
  final events = <NetworkQualityEvent>[];
  final sub = CatcherQuality.subscribe(serverUri, intervalMs: 500,
    onEvent: (event) => events.add(event));
  await Future.delayed(Duration(seconds: 1));
  final countBeforeUnsub = events.length;
  await sub.unsubscribe();
  await Future.delayed(Duration(seconds: 2));
  expect(events.length, equals(countBeforeUnsub));
});

test('NS04-D03: stream interface', () async {
  final stream = CatcherQuality.subscribeAsStream(serverUri, intervalMs: 500);
  final sub = stream.listen((event) { ... });
  await Future.delayed(Duration(seconds: 2));
  await sub.cancel(); // 内部调用 quality_unsubscribe
});
```

---

## N-01: Multipart 测试

### 方案 A（P2）：Dart 侧 encoder

```dart
// packages/catcher_core/test/multipart_encoder_test.dart

test('NS01-D01: encode single text field', () {
  final builder = MultipartEncoder();
  builder.addText('username', 'alice');
  final (body, contentType) = builder.build();
  expect(contentType, contains('multipart/form-data'));
  expect(body, contains('username'));
  expect(body, contains('alice'));
});

test('NS01-D02: encode text + single file', () {
  final imageBytes = Uint8List.fromList([0x89, 0x50, 0x4E, 0x47]); // PNG header
  final builder = MultipartEncoder();
  builder.addText('description', 'photo');
  builder.addFile('file', 'test.png', imageBytes, 'image/png');
  final (body, contentType) = builder.build();
  expect(contentType, contains('boundary='));
  expect(body, contains('test.png'));
  expect(body, contains('image/png'));
});

test('NS01-D03: upload via client.post', () async {
  final fileBytes = Uint8List.fromList([1, 2, 3, 4]);
  final client = CatcherHttpClient(baseUrl: serverUri);
  final encoder = MultipartEncoder();
  encoder.addText('field', 'value');
  encoder.addFile('file', 'data.bin', fileBytes, 'application/octet-stream');
  final (body, contentType) = encoder.build();
  final resp = await client.post('/upload', body, contentType: contentType);
  expect(resp.status, equals(200));
});
```

### 方案 B（P3）：Rust MultipartBuilder

```rust
// packages/catcher-http/src/multipart/builder.rs #[cfg(test)]

#[test]
fn mp01_builder_empty() {
    let (body, ct) = MultipartBuilder::new().build();
    // body 为空或只有结束 boundary
}

#[test]
fn mp02_add_text_produces_valid_body() {
    let mut b = MultipartBuilder::new();
    b.add_text("name", "value");
    let (body, ct) = b.build();
    assert!(ct.starts_with("multipart/form-data; boundary="));
    assert!(body.windows(b"name".len()).any(|w| w == b"name"));
    assert!(body.windows(b"value".len()).any(|w| w == b"value"));
}

#[test]
fn mp03_add_file_produces_filename_header() {
    let mut b = MultipartBuilder::new();
    b.add_file("file", "hello.txt", b"hello".to_vec(), "text/plain");
    let (body, _) = b.build();
    assert!(body.windows(b"hello.txt".len()).any(|w| w == b"hello.txt"));
    assert!(body.windows(b"text/plain".len()).any(|w| w == b"text/plain"));
}

#[test]
fn mp04_multiple_parts() {
    let mut b = MultipartBuilder::new();
    b.add_text("a", "1");
    b.add_file("f", "x.bin", vec![0, 1, 2], "application/octet-stream");
    b.add_text("b", "2");
    let (body, _) = b.build();
    // 验证所有 3 个 part 都在 body 中
    // 验证 boundary 分隔符出现 4 次（开始 + 中间 + 中间 + 结束）
}

#[test]
fn mp05_boundary_not_in_data() {
    // 验证 body 中包含的文本不偶然匹配 boundary 字符串
}

#[test]
fn mp06_content_type_header_format() {
    let mut b = MultipartBuilder::new();
    b.add_text("k", "v");
    let (_, ct) = b.build();
    assert!(ct.starts_with("multipart/form-data; boundary="));
    // boundary 不含引号（或含引号，取决于 RFC 实现）
}

#[test]
#[ignore = "10MB allocation, run manually or in dedicated large-memory CI runner"]
fn mp07_large_binary_file() {
    let data = vec![0u8; 10 * 1024 * 1024]; // 10MB
    let mut b = MultipartBuilder::new();
    b.add_file("big", "big.bin", data, "application/octet-stream");
    let (body, _) = b.build();
    assert!(body.len() > 10 * 1024 * 1024);
}
```

### FFI C ABI 层 — `multipart_test.rs`（新增）

```rust
// packages/catcher-ffi/tests/multipart_test.rs

#[tokio::test]
async fn m01_multipart_create_destroy() {
    let handle = unsafe { catcher_multipart_create() };
    assert!(!handle.is_null());
    unsafe { catcher_multipart_destroy(handle); }
}

#[tokio::test]
async fn m02_add_text_and_build() {
    let handle = unsafe { catcher_multipart_create() };
    let name = ffi_string("field");
    let value = ffi_string("value");
    unsafe { catcher_multipart_add_text(handle, name, value); }

    let mut body_ptr: *mut u8 = std::ptr::null_mut();
    let mut body_len: usize = 0;
    let mut ct_ptr: *mut c_char = std::ptr::null_mut();
    unsafe { catcher_multipart_build(handle, &mut body_ptr, &mut body_len, &mut ct_ptr); }
    assert!(body_len > 0);
    let ct = unsafe { read_c_string(ct_ptr) };
    assert!(ct.starts_with("multipart/form-data"));
    // free body
    unsafe { catcher_free_data(body_ptr as *mut c_void, body_len); }
    unsafe { catcher_multipart_destroy(handle); }
}

#[tokio::test]
async fn m03_add_file_and_build() {
    // add_file("file", "test.txt", b"hello", "text/plain") → build
    // 验证 body 包含 "test.txt" 和 "text/plain"
}

#[tokio::test]
async fn m04_empty_builder_build() {
    let handle = unsafe { catcher_multipart_create() };
    let mut body_ptr: *mut u8 = std::ptr::null_mut();
    let mut body_len: usize = 0;
    let mut ct_ptr: *mut c_char = std::ptr::null_mut();
    unsafe { catcher_multipart_build(handle, &mut body_ptr, &mut body_len, &mut ct_ptr); }
    // 空 builder 不 crash，body 可为空
    unsafe { catcher_multipart_destroy(handle); }
}

#[tokio::test]
async fn m05_build_then_add_after_build() {
    // build() 后继续 add → build() 第二次 → 两次结果独立
}

#[tokio::test]
async fn m06_destroy_null_handle() {
    unsafe { catcher_multipart_destroy(std::ptr::null_mut()); }
    // 不 crash
}
```

---

## 测试覆盖率目标

| 层级 | N-01 | N-02 | N-03 | N-04 |
|------|:----:|:----:|:----:|:----:|
| Rust 核心逻辑 | 10 (方案B) | 8 | 7 | 9 |
| FFI C ABI 边界 | 8 (方案B) | 7 | 7 | 6 |
| Dart 绑定层 | 3 (方案A) | 2 | 5 | 3 |
| **总计** | **11~18** | **17** | **19** | **18** |

---

## CI 集成

所有 Rust FFI 测试通过 `cargo test -p catcher-ffi --test http_test --test quality_test --test multipart_test` 运行。

### 新增 CI Step

```yaml
# .github/workflows/ci.yml 新增 job
native-gap-tests:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test -p catcher-http --lib          # Rust 核心逻辑
    - run: cargo test -p catcher-ffi --test http_test
    - run: cargo test -p catcher-ffi --test quality_test   # 新增
    - run: cargo test -p catcher-ffi --test multipart_test # 新增 (P3)
```

Dart 测试需要编译 Rust FFI 库 + Dart SDK，可在现有 CI 基础上增加：

```yaml
dart-native-gap-tests:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dart-lang/setup-dart@v1
    # Dart FFI 测试需要编译好的 .so/.dylib
    - run: cargo build -p catcher-ffi --release
    - run: cd packages/catcher_core && CATCHER_FFI_PATH=../../target/release/libcatcher_ffi.so dart test test/http_client_test.dart
    - run: cd packages/catcher_core && CATCHER_FFI_PATH=../../target/release/libcatcher_ffi.so dart test test/quality_test.dart
```

> `CATCHER_FFI_PATH` 是已知阻塞项（TEST-02），需在实现阶段解决路径自动发现。

---

## 与现有测试缺口的关系

本测试方案填补了 `11-testing.md` 中以下缺口：

| 现有缺口 | 本次新增 | 说明 |
|---------|---------|------|
| TEST-01 FFI C ABI 零测试 | +14 N-02/N-03 FFI tests | `http_test.rs` 从 7 个扩展到 ~21 个 |
| TEST-01 FFI C ABI 零测试 | +6 N-04 FFI tests | `quality_test.rs` 新增独立文件 |
| TEST-01 FFI C ABI 零测试 | +8 N-01 FFI tests (P3) | `multipart_test.rs` 新增独立文件 |
| TEST-09 Dart 仅测序列化 | +7 N-03/N-02 Dart tests | `http_client_test.dart` 扩展 |
| TEST-09 Dart 仅测序列化 | +3 N-04 Dart tests | `quality_test.dart` 扩展 |
| Rust 核心逻辑 | +34 Rust unit tests | `http_client.rs` / `network_quality.rs` 扩展 |
