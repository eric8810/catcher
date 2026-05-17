# Bug: `catcher_http_multipart` 内存泄露

**严重程度**: 🔴 Medium — 每次调用泄露 8 字节（`Box<usize>`，累积式）

**状态**: Open

**位置**: `packages/catcher-http/src/ffi/http_ffi.rs:455-457`

---

## 当前代码

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_http_multipart(
    handle: *mut c_void,
    ...
) {
    if handle.is_null() { return; }
    let id = Box::from_raw(handle as *mut usize);
    let id_val = *id;
    std::mem::forget(id); // don't drop, handle stays valid
    // ...
}
```

## 问题

`Box::from_raw` 从原始指针重建了 `Box<usize>`，获取了堆上 8 字节的所有权。随后 `mem::forget(id)` 吞没了这个 `Box` 而不释放其内存，导致这 8 字节永久泄露。

每次调用 `catcher_http_multipart` 泄露一次。

## 根因

这段代码想"读取 handle 指针指向的值但不释放它"。正确做法是直接解引用原始指针，不需要 `from_raw` + `forget`。其他所有 FFI 函数（如 `catcher_http_get`、`catcher_ws_send_text` 等）都正确使用了 `*(handle as *const usize)`，唯独 `catcher_http_multipart` 用了错误的模式。

## 修复

```rust
// ❌ 当前：leak
let id = Box::from_raw(handle as *mut usize);
let id_val = *id;
std::mem::forget(id);

// ✅ 修复：与其他函数一致
let id_val = *(handle as *const usize);
```

## 影响范围

- 仅 `catcher_http_multipart` 一个函数
- `handle` 本身对应的原始 `Box<usize>` 由 `catcher_http_client_destroy` 管理，此处不应获取其所有权

## 关联

- 规范：AGENTS.md — FFI 规则 "禁止 `Box::from_raw` + `mem::forget` 误用"
