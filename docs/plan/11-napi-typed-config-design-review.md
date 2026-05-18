# 11-napi-typed-config-design 评审记录

> 评审对象：`docs/plan/11-napi-typed-config-design.md`（已修正，976 行）
> 评审日期：2025-01-20
> 状态：全部修正 ✓

---

## 严重错误（已修正 ✓）

### E1: `WsEvent.Message` 的 `data` 字段名应为 `data_base64`

- **位置**：`ts/types.ts` 第 476 行
- **现状**：`{ type: 'Message'; data: string; is_binary: boolean }`
- **应为**：`{ type: 'Message'; data_base64: string; is_binary: boolean }`
- **依据**：`catcher-ws/src/types/ws.rs:24-28`，`WsEvent::to_ffi_json()` 将 `data: Vec<u8>` 做 base64 编码后以 `data_base64` 字段名输出
- **影响**：用户解构 `event.data` 拿到 `undefined`

---

### E2: `SseClientConfig.reconnect` 与 Rust 不对齐

- **位置**：`ts/types.ts` 第 359-364 行
- **现状**：
  ```typescript
  reconnect?: {
    enabled?: boolean          // ❌ Rust 中不存在
    max_retries?: number
    initial_delay_ms?: number
    max_delay_ms?: number
    // ❌ 缺少 backoff_multiplier
  }
  ```
- **应为**：
  ```typescript
  reconnect?: {
    max_retries?: number       // 默认: 10
    initial_delay_ms?: number  // 默认: 1000
    max_delay_ms?: number      // 默认: 30000
    backoff_multiplier?: number // 默认: 2.0
  }
  ```
- **依据**：`catcher-core/src/types/sse.rs:53-61`，`SseReconnectConfig` 有 4 个字段，无 `enabled`
- **影响**：用户传入 `enabled` 被静默忽略，传入 `backoff_multiplier` 无法生效

---

### E3: 多处默认值与 Rust 源码不一致

| 字段 | types.ts 声称 | Rust 实际 | Rust 文件:行 |
|------|-------------|----------|-------------|
| `RetryConfig.min_backoff_ms` | 500 | **100** | `resilience.rs:43` |
| `RetryConfig.max_backoff_ms` | 30000 | **10000** | `resilience.rs:46` |
| `RetryConfig.jitter` | false | **true** | `resilience.rs:48` |
| `CircuitBreakerConfig.half_open_max_requests` | 1 | **5** | `resilience.rs:93` |

- **影响**：注释中的错误默认值会直接误导用户

---

## 中等遗漏（已修正 ✓）

### M1: `SseClientConfig.circuit_breaker` 内联类型不完整

- **位置**：`ts/types.ts` 第 366-369 行
- **现状**：
  ```typescript
  circuit_breaker?: {
    failure_threshold?: number
    reset_timeout_ms?: number
    // 缺少 success_threshold 和 half_open_max_requests
  }
  ```
- **建议**：要么复用已有的 `CircuitBreakerConfig` interface（第 273 行），要么补齐 4 个字段
- **依据**：Rust `CircuitBreakerConfig`（`resilience.rs:66-82`）有 4 个字段

---

### M2: `TlsConfig` 缺少 7 个字段

- **位置**：`ts/types.ts` 第 228-243 行
- **现状**：只有 7 个字段
- **Rust 实际字段**（`http.rs:115-155`）：共 14 个，缺少：
  - `client_identity_pfx` — PFX/PKCS12 客户端身份
  - `client_identity_password` — PFX 身份密码
  - `tls_sni_override` — SNI 覆写
  - `min_tls_version` — 最低 TLS 版本
  - `max_tls_version` — 最高 TLS 版本
  - `pin_sha256` — SHA-256 公钥指纹 pinning
- **建议**：如果 napi 未使用这些字段，在注释中说明"仅列出 napi 当前支持的 TLS 字段"及遗漏原因

---

### M3: `ts/client.ts` HTTP wrapper 丢失方法可用性检查

- **位置**：`ts/client.ts` 第 520-530 行（`put` / `delete` / `patch`）
- **现状**：直接调用 `this._raw.put(...)`，不检查方法是否存在
- **当前 `client.js` 行为**（第 61-72 行）：
  ```javascript
  async put(url, body, options) {
    if (this._raw.put) return this._raw.put(...)
    throw new Error('put() requires rebuilt native addon (cargo build)')
  }
  ```
- **影响**：用户使用旧版 `.node` 二进制（缺少 `put` / `patch`）时，将得到 `this._raw.put is not a function` 而非清晰的错误提示
- **建议**：保留 guard 或说明放弃兼容旧版二进制的理由

---

## 低风险问题（已处理 ✓）

### L1: `tsup` 多入口构建中 `native.ts` 的加载行为

- 当前 `build:ts` 命令同时构建 3 个入口：`ts/client.ts ts/types.ts ts/sse.ts`
- `client.ts` 和 `sse.ts` 都 `import { loadNativeAddon } from './native'`
- tsup 对每个入口做独立 bundle，`native.ts` 逻辑会被内联到 `client.js` 和 `sse.js` 各一份
- **结论**：运行时正确（两份副本独立运行），但 `dist/` 中无独立的 `native.js`，确认此为预期行为即可
- `types.ts` 只有 `export type` 声明，编译后 `types.js` 为空文件也是正常现象

---

### L2: `package.json` 的 `files` 与构建流程依赖

- `"files": ["dist/", "index.js", "index.d.ts", "*.node", "npm/"]`
- `dist/` 由 CI 构建产生，`.gitignore` 中排除
- 需确保 CI 中 `npm run build`（含 `build:ts`）在 `npm publish` 之前执行
- Phase 3 CI 集成已覆盖此依赖链，确认无误

---

## 建议（已纳入 Phase 4 ✓）

### S1: napi WS ThreadsafeFunction Blocking 模式死锁风险

- `catcher-napi-ws/src/lib.rs:82` 使用 `ThreadsafeFunctionCallMode::Blocking`
- 用户如果在 WS 事件回调内部同步调用 `send()`，可能因 napi-rs 单线程限制导致死锁
- **建议**：在类型注释或文档中提醒用户避免在事件回调内同步调用 `send()`，或在 wrapper 中使用 `setImmediate` / `process.nextTick` 延迟发送

---

### S2: `default_true()` / `default_false()` 去重

- 当前代码在 3 个 crate 中重复定义 `default_true()`：
  - `packages/catcher-http/src/types/http.rs:86`
  - `packages/catcher-ws/src/types/ws.rs:102`
  - `packages/catcher-core/src/types/resilience.rs:48`
- AGENTS.md 规定须从 `catcher-core` 公共模块导入，禁止各 crate 重复定义
- Phase 2 大量改 Rust 代码时是修复此问题的最佳时机

---

### S3: `serde_json::json!` 宏使用违反编码规范

- `packages/catcher-napi-http/src/sse.rs:179-187` 三处使用 `serde_json::json!`
- RUST_STYLE_GUIDE 规定：需 JSON 序列化的 enum 统一使用 `#[serde(tag = "type")]`，禁止手动 `serde_json::json!` 构建
- 建议在 Phase 1 或后续重构中改为 tagged enum 序列化
