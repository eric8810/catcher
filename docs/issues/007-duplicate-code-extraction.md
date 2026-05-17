# Style: 跨 crate 重复代码抽取

**严重程度**: 🟡 Medium — 维护负担：修改一处需要同步多处

**状态**: Open

---

## 重复清单

### P0 — 简单函数重复

| 函数 | 出现次数 | 文件 |
|------|----------|------|
| `error_json(msg: &str) -> String` | 4 | `http_ffi.rs`, `sse_ffi.rs`, `ws_ffi.rs`, `quality_ffi.rs` |
| `default_true() -> bool` | 3 | `types/http.rs`, `types/resilience.rs`, `types/ws.rs` |

**修复**：`error_json` → `catcher_core::ffi_types`；`default_true` / `default_false` → `catcher_core::utils`

### P1 — 中等重复

| 函数 | 出现次数 | 文件 |
|------|----------|------|
| `ffi_string_to_string(s: FfiString, default: &str) -> String` | 4 | `http_ffi.rs`, `sse_ffi.rs`, `ws_ffi.rs`, `quality_ffi.rs` |
| `read_body_bytes(body, body_len) -> Vec<u8>` | 2 | `http_ffi.rs`, `sse_ffi.rs` |
| `parse_headers_json(*const c_char) -> HashMap` | 2 | `http_ffi.rs`, `sse_ffi.rs` |
| `invoke_*_callback(...)` | 4 | `http_ffi.rs`, `sse_ffi.rs`, `ws_ffi.rs`, `quality_ffi.rs` |

**修复**：
- `ffi_string_to_string` — `FfiString` 已有同名方法 `to_string_lossy()`，直接用即可
- `read_body_bytes` / `parse_headers_json` → `catcher_core::ffi_types`
- `invoke_*_callback` → 统一为 `catcher_core::ffi_types::invoke_callback`

### P2 — 结构重复

| 模式 | 出现次数 | 文件 |
|------|----------|------|
| Handle 注册表 `Mutex<Option<HashMap<usize, Arc<T>>>>` | 3 | `http_ffi.rs`, `ws_ffi.rs`, `sse_ffi.rs` |
| 全局 `OnceLock<Runtime>` | 5 | `http_ffi.rs`, `sse_ffi.rs`, `ws_ffi.rs`, `quality_ffi.rs`, `uniffi/lib.rs` |

**修复**：泛型 `HandleRegistry<T>` + `catcher_core::runtime::global_runtime()` 公共函数

---

## 目标模块

```
catcher-core/src/
  utils.rs         — default_true(), default_false()
  ffi_helpers.rs   — error_json(), read_body_bytes(), parse_headers_json(), invoke_callback()
  handle_registry.rs — HandleRegistry<T>
  runtime.rs       — global_runtime()
```

## 关联

- 规范：AGENTS.md — "禁止在多个 crate 中重复定义相同函数"
- 性能：见 `006-handle-registry-lock-contention.md`
