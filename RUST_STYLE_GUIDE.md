# Rust 代码规范 — catcher 项目

> 基于对全部 58 个非测试 Rust 源文件的完整阅读分析生成。
> 最后更新：2025-07-10

---

## 目录

1. [注释规范](#1-注释规范)
2. [命名规范](#2-命名规范)
3. [代码结构](#3-代码结构)
4. [类型与数据结构](#4-类型与数据结构)
5. [错误处理](#5-错误处理)
6. [并发与异步](#6-并发与异步)
7. [FFI 规范](#7-ffi-规范)
8. [测试规范](#8-测试规范)
9. [性能与内存](#9-性能与内存)
10. [合规统计](#10-合规统计)
11. [问题清单](#11-问题清单)

---

## 1. 注释规范

### 1.1 注释语言

| 规则 | 优先级 |
|------|--------|
| 模块级文档注释 (`//!`) 和公共 API 文档注释 (`///`) 统一使用**中文** | **必须** |
| 内部实现注释 (`//`) 可使用中文或英文，但同一文件内保持一致 | 推荐 |
| FFI 导出函数的文档注释可使用英文（面向跨语言调用方） | 允许例外 |

### 1.2 文档注释格式

```rust
// ✅ 正确：/// 后有一个空格，段落间空行
/// 根据网络质量返回建议的并发数。
///
/// Excellent → 50, Good → 25, Fair → 10,
/// Poor → 5, Bad → 2
pub fn concurrency_for_quality(level: NetworkQualityLevel) -> usize { ... }

// ❌ 错误：多行堆叠无空行
/// 根据网络质量返回建议的并发数
/// Excellent → 50, Good → 25
pub fn concurrency_for_quality(...) -> usize { ... }

// ❌ 错误：/// 后无空格
///HTTP 方法枚举
pub enum HttpMethod { ... }
```

### 1.3 区段分隔线

统一使用 `// ──`（两个短横，两端有空格）：

```rust
// ── Lifecycle ──

// ── Internal Helpers ──
```

禁止使用 `// ═══`（双线等号），禁止无横线纯文字分隔。

### 1.4 禁止的注释类型

| 禁止项 | 说明 | 示例 |
|--------|------|------|
| 阶段追踪注释 | 开发期临时标记，合入主干前移除 | `// Phase 3: retry middleware` |
| 需求编号注释 | 同上 | `// N-03: per-request cancel` |
| 游离 TODO/FIXME | 缺少 issue 号或负责人 | 须写成 `// TODO(#123): desc` |

---

## 2. 命名规范

### 2.1 基本规则

| 元素 | 风格 | 示例 |
|------|------|------|
| 类型 (struct/enum/trait) | `PascalCase` | `HttpTransport`, `CircuitBreaker` |
| 函数/方法 | `snake_case` | `build_request`, `on_pong` |
| 变量 | `snake_case` | `rtt_window`, `request_id` |
| 常量/静态 | `SCREAMING_SNAKE_CASE` | `CHARSET`, `MAX_RETRIES` |
| 模块文件 | `snake_case` | `http_client.rs`, `ws_ffi.rs` |

### 2.2 缩写策略

| 上下文 | 规则 | 说明 |
|--------|------|------|
| 公开 API 名称 | **禁止缩写** | `HeartbeatManager` 而非 `HbMgr` |
| 局部变量 (≤10 行作用域) | 允许常见缩写 | `req`, `resp`, `cfg`, `ctx` |
| 字段名 (struct) | **禁止缩写** | `circuit_breaker` 而非 `cb` |

### 2.3 平台前缀

| 前缀 | 用途 | 示例 |
|------|------|------|
| `Js*` | napi JavaScript 绑定类型 | `JsHttpClient`, `JsHttpResponse` |
| `Ffi*` | C ABI 兼容类型 | `FfiResult`, `FfiString`, `FfiBytes` |

---

## 3. 代码结构

### 3.1 模块组织

```
packages/{crate}/src/
  lib.rs           — 仅 pub mod + pub use re-export，不含逻辑
  types/
    mod.rs         — pub mod http / pub mod ws
    http.rs        — 类型定义
  transport/       — 网络 I/O 层
  resilience/      — 重试 / 熔断 / 超时
  scheduler/       — 优先级队列 / 并发控制
  observability/   — 指标 / 网络质量
  sse/             — SSE 客户端 / 流
  ffi/             — C ABI 导出
```

### 3.2 文件大小

| 规则 | 阈值 |
|------|------|
| 单个源文件建议行数 | ≤ 500 行 |
| 单个源文件最大行数 | ≤ 800 行 |
| 单函数建议行数 | ≤ 80 行 |

> 当前超标文件：`http_client.rs`（1047 行）、`http_ffi.rs`（519 行）、`ws_client.rs`（518 行）。

### 3.3 `use` 导入顺序

遵循 4 组顺序，组间空行，组内按字母排序：

```rust
// 1. std
use std::collections::HashMap;
use std::sync::Arc;

// 2. 第三方
use reqwest::Client;
use serde::{Deserialize, Serialize};

// 3. catcher_core
use catcher_core::CatcherError;

// 4. crate
use crate::types::http::HttpClientConfig;
```

**禁止**模块级 `use foo::*;`（`http_client.rs:17` 当前违规）。

---

## 4. 类型与数据结构

### 4.1 Config 结构体标准模板

所有配置结构体**必须**遵循此模板：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FooConfig {
    #[serde(default = "default_bar")]
    pub bar: u64,
    #[serde(default = "default_baz")]
    pub baz: String,
}

fn default_bar() -> u64 { 42 }
fn default_baz() -> String { "hello".into() }

impl Default for FooConfig {
    fn default() -> Self {
        Self {
            bar: default_bar(),
            baz: default_baz(),
        }
    }
}
```

- 每个字段名对应的默认函数命名为 `default_<field_name>`
- `default_true()` / `default_false()` 须从 `catcher-core` 公共模块导入，**禁止**在多 crate 中重复定义

### 4.2 Enum JSON 序列化

对于需要 JSON 序列化的 enum，**统一使用** `#[serde(tag = "type")]`：

```rust
// ✅ 推荐
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    Headers { status: u16, headers: HashMap<String, String> },
    Chunk(Vec<u8>),
    Done,
    Error(String),
}

// ❌ 避免：手动 serde_json::json! 构建
match event {
    StreamEvent::Headers { status, headers } => {
        serde_json::json!({"type": "Headers", "status": status, ...})
    }
}
```

### 4.3 内部可见性

- 模块内部类型/函数使用 `pub(crate)` 而非 `pub`
- 仅在 `lib.rs` 中通过 `pub use` 暴露公开 API

---

## 5. 错误处理

### 5.1 错误类型层级

```
catcher_core::CatcherError     ← 所有 Rust 代码的唯一错误类型
  ├── napi::Error              ← napi 绑定层薄封装
  └── uniffi::CatcherError     ← UniFFI 绑定层薄封装
```

- 新增 `CatcherError` 变体须同步更新 `CatcherError::category()` 方法
- 各绑定层定义自己的薄封装 Error，不直接暴露 `CatcherError` 给平台调用方

### 5.2 错误转换

```rust
// ✅ 标准模式
.map_err(|e| CatcherError::Internal(format!("context: {e}")))?;

// ✅ FFI 标准模式
.map_err(|e| napi::Error::from_reason(e.to_string()))?;
```

### 5.3 禁止的行为

- **禁止** `unwrap()` 在非测试代码中（除非有不可恢复的语义，须注释说明）
- **禁止** `unwrap_or_default()` 掩盖 `CString::new` 失败（含 null 字节的字符串是编程错误，应 `expect`）
- **禁止** 吞没错误（`let _ = ...`），除非明确注释原因

---

## 6. 并发与异步

### 6.1 全局 Runtime

统一使用 `std::sync::OnceLock<tokio::runtime::Runtime>` 模式：

```rust
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-xxx")
    })
}
```

> **待办**：当前 4 个 FFI 模块 + uniffi 各自定义此函数，应抽取到 `catcher-core` 公共模块。

### 6.2 Handle 注册表

FFI 层的 handle 注册表**应**抽取为 `catcher-core` 中的泛型 `HandleRegistry<T>`：

```rust
// 目标形态（待实现）
use catcher_core::HandleRegistry;
static HTTP_HANDLES: HandleRegistry<HttpTransport> = HandleRegistry::new();
```

> 当前 `std::sync::Mutex<Option<HashMap<usize, Arc<T>>>>` 在 3 个 FFI 模块中重复实现。

### 6.3 Atomic Ordering

| 场景 | Ordering |
|------|----------|
| 纯统计计数器（MetricsCollector） | `Relaxed` |
| 状态机字段（CircuitBreaker） | `AcqRel` |
| ID 生成（next_request_id） | `Relaxed` |

> 当前 `CircuitBreaker` 的 `failure_count`、`success_count` 等状态计数器使用 `Relaxed`，建议审计后改为 `AcqRel`。

### 6.4 共享状态

- 读多写少场景：使用 `Arc<tokio::sync::RwLock<T>>` 而非 `Arc<Mutex<T>>`
- 高并发计数：使用 `AtomicU64` + `Relaxed`
- 禁止在 `.await` 持有 `std::sync::MutexGuard`（当前代码已正确规避）

---

## 7. FFI 规范

### 7.1 函数签名

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_xxx(
    handle: *mut c_void,
    input: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) -> FfiResult { ... }
```

### 7.2 必做检查清单

- [ ] 入口处检查所有指针是否为 null
- [ ] `CStr::from_ptr` 前确保指针非 null
- [ ] `CString::new` 前 `replace('\0', "")` 去除 null 字节
- [ ] `into_raw()` 转移所有权的指针须在文档中标注释放责任方
- [ ] 回调中的 CString 须配对 `catcher_free_event_data()` 释放

### 7.3 公共辅助函数

以下函数当前在多个 FFI 模块中重复定义，应抽取：

| 函数 | 出现位置 | 应归入 |
|------|----------|--------|
| `error_json(msg: &str) -> String` | `http_ffi`, `sse_ffi`, `ws_ffi`, `quality_ffi` | `catcher_core::ffi_types` |
| `ffi_string_to_string(s: FfiString, default: &str) -> String` | `http_ffi`, `sse_ffi`, `ws_ffi`, `quality_ffi` | `catcher_core::FfiString` (已存在方法) |
| `read_body_bytes(body, body_len) -> Vec<u8>` | `http_ffi`, `sse_ffi` | `catcher_core::ffi_types` |
| `parse_headers_json(*const c_char) -> HashMap` | `http_ffi`, `sse_ffi` | `catcher_core::ffi_types` |
| `invoke_*_callback(...)` | `http_ffi`, `sse_ffi`, `quality_ffi`, `ws_ffi` | `catcher_core::ffi_types` |

---

## 8. 测试规范

### 8.1 测试组织

- 单元测试：`#[cfg(test)] mod tests { }` 置于对应源文件末尾
- 集成测试：crate 根目录 `tests/` 目录（当前未使用，保留）

### 8.2 测试命名

```
<被测函数/场景>_<条件>_<期望结果>
```

```rust
// ✅ 推荐
#[test]
fn heartbeat_pong_resets_missed_count() { ... }

#[test]
fn connect_returns_error_on_invalid_url() { ... }

// ❌ 禁止编号式
#[test]
fn test_14_retry_zero() { ... }

// ❌ 禁止模糊描述
#[test]
fn rc1_basic_consumption() { ... }
```

### 8.3 测试辅助

- 复用配置使用 `fn test_config() -> XxxConfig` helper
- HTTP mock 使用 `wiremock`
- 异步测试使用 `#[tokio::test]`


