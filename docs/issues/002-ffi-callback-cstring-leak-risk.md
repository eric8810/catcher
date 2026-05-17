# Risk: FFI 回调 CString 所有权泄漏风险

**严重程度**: 🟡 Low-Medium — 取决于调用方是否正确释放

**状态**: Open

**位置**: 4 处 `invoke_*_callback()` + `QualitySubscription::start()`

| 函数 | 文件 |
|------|------|
| `invoke_http_callback` | `packages/catcher-http/src/ffi/http_ffi.rs:54-61` |
| `invoke_sse_callback` | `packages/catcher-http/src/ffi/sse_ffi.rs:57-68` |
| `invoke_quality_callback` | `packages/catcher-http/src/ffi/quality_ffi.rs:32-43` |
| `invoke_event_callback` | `packages/catcher-ws/src/ffi/ws_ffi.rs:41-57` |
| `QualitySubscription::start` | `packages/catcher-http/src/observability/network_quality.rs:176-184` |

---

## 模式

```rust
fn invoke_http_callback(callback: EventCallback, event_name: &str, json: String, user_data: usize) {
    let c_event = CString::new(event_name.replace('\0', "")).unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
    let json_len = c_json.as_bytes().len();
    callback(
        c_event.into_raw(),         // ← 所有权转移给调用方
        c_json.into_raw() as *const u8,  // ← 同上
        json_len,
        user_data as *mut c_void,
    );
}
```

## 问题

`CString::into_raw()` 将所有权转移给接收方。FFI 调用方（Dart/Swift/Kotlin）**必须**在回调中调用 `catcher_free_event_data(event_type, event_data)` 来释放这两个 CString。

如果调用方未正确调用释放函数，每次事件回调泄漏 2 个 CString（event_name + json）。

## 当前缓解

- `catcher_free_event_data()` 已在 `catcher-core/src/ffi_types.rs:119-132` 实现
- 文档注释已标注所有权转移

## 建议

1. 在 Dart/Kotlin/Swift 绑定中确认每个回调实现都调用了 `catcher_free_event_data`
2. 考虑在回调接口层增加自动释放包装（如 Dart 的 `NativeFinalizer`）
3. 这 5 处 `invoke_*_callback` 函数实质相同，可抽取到 `catcher_core::ffi_types` 减少重复

## 关联

- 规范：AGENTS.md — FFI 规则 "`into_raw()` 转移所有权后必须在文档中标注调用方释放责任"
- 重复代码：RUST_STYLE_GUIDE.md 附录 B
