# Bug: `catcher_free_data` UB — CString 数据用错误的 capacity 重建 Vec

**严重程度**: 🔴 High — Undefined Behavior（heap corruption risk）

**状态**: Open

**位置**: `packages/catcher-ffi/src/lib.rs:91-96`

---

## 当前代码

```rust
/// Free data allocated by catcher_pack / catcher_unpack.
#[no_mangle]
pub unsafe extern "C" fn catcher_free_data(data: *mut c_void, len: usize) {
    if data.is_null() { return; }
    // Reconstruct as Vec<u8> and drop — works for both Box<[u8]> and CString
    let _ = Vec::from_raw_parts(data as *mut u8, len, len);
}
```

## 问题：两种分配来源但一种释放方式

| 来源 | 分配方式 | 实际 capacity | `catcher_free_data` 使用的 capacity |
|------|---------|---------------|-------------------------------------|
| `catcher_pack` | `Box<[u8]>` → `into_raw()` | `len` ✅ | `len` ✅ |
| `catcher_unpack` | `CString` → `into_raw()` | `len + 1` ❌ | `len` ❌ |

`CString::into_raw()` 返回的指针指向 **len+1** 字节的堆分配（包含 null terminator）。`catcher_unpack` 返回的 `data_len = len` 是字符串长度（**不含** null）。

`Vec::from_raw_parts(ptr, len, len)` 创建的 Vec 认为 capacity=len。drop 时，分配器被通知释放 `Layout::array::<u8>(len)` 字节，但实际分配的是 `Layout::array::<u8>(len + 1)` 字节。

**这是 Undefined Behavior** — 分配器 metadata 与实际分配大小不匹配，可能导致 heap corruption。

## 修复

区分两种来源：

```rust
pub unsafe extern "C" fn catcher_free_data(data: *mut c_void, len: usize, is_cstring: bool) {
    if data.is_null() { return; }
    if is_cstring {
        // CString: real capacity is len + 1 (with null terminator)
        let _ = CString::from_raw(data as *mut c_char);
    } else {
        // Box<[u8]>: real capacity is len
        let _ = Vec::from_raw_parts(data as *mut u8, len, len);
    }
}
```

或者更彻底：让 `FfiResult::drop()` 也负责释放 `data` 字段（区分 error_message 和 data 的释放方式），消除调用方手动管理两个释放路径的需要。

## 影响

- 任何调用 `catcher_unpack` 并通过 `catcher_free_data` 释放的路径都会触发 UB
- `catcher_pack` 路径不受影响（Box<[u8]> 布局与 Vec 兼容）
