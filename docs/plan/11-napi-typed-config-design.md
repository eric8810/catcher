# 11 — napi TS Wrapper 实施计划

> 架构设计文档：[`arch-rs/16-napi-ts-wrapper.md`](../arch-rs/16-napi-ts-wrapper.md)
> Review 记录：[`11-napi-typed-config-design-review.md`](./11-napi-typed-config-design-review.md)

---

## 变更清单

### Phase 1 — TypeScript wrapper 重写（核心，低风险，不改 Rust）

| 文件 | 变更 |
|------|------|
| `packages/catcher-napi-http/ts/types.ts` | **新建** — HTTP/SSE 配置和事件类型 |
| `packages/catcher-napi-http/ts/native.ts` | **新建** — 原生模块加载 |
| `packages/catcher-napi-http/ts/client.ts` | **新建** — HttpClient wrapper |
| `packages/catcher-napi-http/ts/sse.ts` | **新建** — SseStream / SseClient wrapper |
| `packages/catcher-napi-http/tsconfig.json` | **新建** |
| `packages/catcher-napi-http/package.json` | **修改** — 入口指向 `dist/`，添加 `tsup` 依赖 |
| `packages/catcher-napi-http/client.js` | **删除** |
| `packages/catcher-napi-http/client.d.ts` | **删除** |
| `packages/catcher-napi-ws/ts/types.ts` | **新建** — WS 配置和事件类型 |
| `packages/catcher-napi-ws/ts/native.ts` | **新建** |
| `packages/catcher-napi-ws/ts/client.ts` | **新建** — WsClient wrapper |
| `packages/catcher-napi-ws/tsconfig.json` | **新建** |
| `packages/catcher-napi-ws/package.json` | **修改** |
| `packages/catcher-napi-ws/client.js` | **删除** |
| `packages/catcher-napi-ws/client.d.ts` | **删除** |

### Phase 2 — camelCase 兼容（需改 Rust，中等风险）

| 文件 | 变更 |
|------|------|
| `packages/catcher-http/src/types/http.rs` | `HttpClientConfig` 及子结构体添加 `#[serde(alias)]` |
| `packages/catcher-ws/src/types/ws.rs` | `WsClientConfig` 及子结构体添加 `#[serde(alias)]` |
| `packages/catcher-core/src/types/sse.rs` | `SseClientConfig` 添加 `#[serde(alias)]` |
| `packages/catcher-core/src/types/resilience.rs` | `RetryConfig` / `CircuitBreakerConfig` 添加 `#[serde(alias)]` |
| `ts/types.ts` | JSDoc 注释中标注 camelCase 别名 |

### Phase 3 — CI 集成

| 文件 | 变更 |
|------|------|
| `.github/workflows/ci.yml` | 添加 `pnpm build:ts` 步骤 |
| `.github/workflows/release.yml` | 确保 `build:ts` 在 napi 构建后执行 |
| `.gitignore` | 添加 `packages/catcher-napi-*/dist/` |

### Phase 4 — Rust 代码规范清理（与 Phase 2 同步进行）

| 问题 | 文件 | 变更 |
|------|------|------|
| `default_true()` 重复定义 | `catcher-http/src/types/http.rs`、`catcher-ws/src/types/ws.rs`、`catcher-core/src/types/resilience.rs` | 统一从 `catcher-core` 公共模块导入（AGENTS.md 规定） |
| SSE `serde_json::json!` 手动构建 | `catcher-napi-http/src/sse.rs:179-187` | 改为 tagged enum `#[serde(tag = "type")]` + derive 序列化（RUST_STYLE_GUIDE 规定） |
| `RetryConfig.backoff` 默认值不一致 | `catcher-core/src/types/resilience.rs:23-24` vs `:56` | `#[serde(default)]` 用 `BackoffKind::default()`（Fixed），`RetryConfig::default()` 显式写 `Exponential`，两者应对齐 |

---

## 实施顺序

```
Phase 1 (TS wrapper)
  ↓
Phase 3 (CI 集成) ← 确保 Phase 1 的构建在 CI 中运行
  ↓
Phase 2 (Rust serde alias) + Phase 4 (Rust 规范清理) ← 同步进行，改 Rust 后再更新 TS 类型注释
```

Phase 1 和 Phase 3 可以合为一次 PR。Phase 2 和 Phase 4 合为另一次 PR。
