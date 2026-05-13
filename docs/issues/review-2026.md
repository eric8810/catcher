# Code Review — 2026-05-12

> 审查范围：所有包（Rust / TS / napi）
> 发现总数：15

---

## 🔴 严重

### 1. Per-request retry 覆盖完全无效

**文件**：`packages/catcher-http-ts/src/http/client.ts:166-169`

`rawDoRequest` 把 `args[args.length - 1]` 传给 `effectiveRetry()`，但这里始终是 `axiosConfig`（仅含 headers/timeout/signal/responseType）。**retry 字段从未拷入 axiosConfig**。

结果：`{ retry: false }` 或 `{ retry: { attempts: 5 } }` 的 per-request 设置完全无作用。

修复：在构建 `axiosConfig` 时加入 retry 字段，或在 `effectiveRetry` 之前从原始 `RequestConfig` 读取。

### 2. onRetry 回调触发两次

**文件**：`packages/catcher-http-ts/src/http/retry.ts:71,75`

```typescript
(options as any).onRetry?.(error.attemptNumber)   // line 71
options.onRetry?.(error.attemptNumber)              // line 75
```

同一个属性用两种方式调了两次。line 71 是早期残留。

### 3. CbState 传递依赖

**文件**：`packages/catcher-napi-http/src/lib.rs:24`

`use catcher_core::types::resilience::CbState` 但 `Cargo.toml` 没有 `catcher-core` 依赖。靠 `catcher-http` 传递依赖编译通过。一旦 `catcher-http` 移除这个 re-export，构建直接 break。

### 4. 无意义的 napi feature flag

**文件**：`packages/catcher-napi-http/Cargo.toml:12`

```toml
catcher-http = { path = "../catcher-http", features = ["napi"] }
```

但 `catcher-http` 的 `napi` feature 只为两个可选依赖（napi/napi-derive）存在，**没有任何 `#[cfg(feature = "napi")]` 条件编译代码**。白白增加 15-30s 编译。

---

## 🟡 中等

### 5. retry JSDoc 与实现矛盾

**文件**：`packages/catcher-http-ts/src/http/retry.ts:10-13`

JSDoc 写"NOT on ETIMEDOUT"，但代码 line 52 包含 `error.code === 'ETIMEDOUT'`。注释过时。

### 6. index.d.ts retry 字段名不一致

**文件**：`packages/catcher-napi-http/index.d.ts`

TS 类型用 `max_attempts`，但 `@eric8810/http` 的 `RetryOptions` 用 `attempts`。应统一。

### 7. 拦截器 eject() 不校验 id

**文件**：`packages/catcher-http-ts/src/http/interceptors.ts`

`eject(id)` 无论 id 是否存在都静默返回。axios 行为相同（设计如此），但值得文档说明。

### 8. @eric8810/web 缺少 tsconfig

**文件**：`packages/catcher-web/`

新建包没有 `tsconfig.json`，无法独立 typecheck。

### 9. package.json 和 pnpm-workspace.yaml 重复

**文件**：`packages/package.json` 和 `pnpm-workspace.yaml`

`packages/package.json` 的 `workspaces` 字段和根 workspaces 部分重叠。非 bug，但增加维护负担。

---

## 🟢 低优先级

### 10. napi-http index.d.ts 中 patch() 返回类型错误

**文件**：`packages/catcher-napi-http/index.d.ts`

`patch()` 声明返回 `void`，但实际返回 `Promise<HttpResponse>`。类型定义错误但不影响 JavaScript 使用。

### 11. TypeScript 类型 `retry: false` 断言无实际效果

**文件**：`packages/catcher-http-ts/src/http/client.ts:166`

类型定义接受 `retry: false`，但 `effectiveRetry()` 把 `false` 转为 `null`，两个值语义相同（都不重试）。类型层面可简化。

### 12-15. 低优杂项

| 问题 | 文件 |
|------|------|
| `index.d.ts` 部分字段在 Rust 侧已废弃但 TS 类型仍保留 | napi-http/index.d.ts |
| JSON config schema doc comment 不完整 | napi-http/src/lib.rs:3-13 |
| 截断的文档字符串 | napi-http/src/lib.rs |
| napi Cargo.toml 缺少 `catcher-core` 显式依赖声明 | napi-http/Cargo.toml |

---

## 修复顺序

```
🔴1 → 🔴2 → 🔴3 → 🔴4 → 🟡5 → 🟡6 → 🟡8 → 🟢10 → 🟡7 → 🟡9
```
