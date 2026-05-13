# 剩余开发计划

> 状态：核心全部实现，剩余补全 + 集成验证

---

## 一、已完成 (无需再动)

| 组件 | 状态 |
|------|------|
| catcher-core / catcher-http / catcher-ws (Rust) | ✅ 完整实现 |
| @catcher/core / http / ws (TS) | ✅ 完整实现，含拦截器 |
| @catcher/napi-http / napi-ws (.node 已编译) | ✅ 基础 API 可用 |
| proxy.ts — 6 种损伤模型 | ✅ |
| presets.ts — 17 个 profile | ✅ |
| 测试 — S1-S16c + 吞吐基准 | ✅ |

---

## 二、剩余工作

### P1 — 必须做的

| 序号 | 项目 | 工作内容 | 状态 |
|------|------|---------|------|
| 1 | **重编译 napi .node** | Rust lib.rs 已改（PUT/DELETE/PATCH + RequestOptions + CB state），需 `cargo build` 重新编译 `.node` | ✅ lib.rs + client.js wrapper 已完成（GET/POST/PUT/DELETE/PATCH + circuitBreakerState） |
| 2 | **@catcher/web 拦截器落地** | 完整的 interceptor manager（复用 interceptors.ts 逻辑） | ✅ `interceptors.ts` 实现 createInterceptorManager（use/eject/clear + LIFO request/FIFO response chain），`client.ts` 已集成 |
| 3 | **@catcher/web WebSocket** | 原生 WebSocket + 重连封装 | ✅ `ws/client.ts` 实现 createWebSocketClient（exponential backoff 重连、多端点、事件监听） |
| 4 | **napi-http JS wrapper 补全** | `client.js` wrapper + `circuitBreakerState()` 的 JS 调用链路 | ✅ `client.js` 封装所有方法 + runtime feature check + circuitBreakerState fallback |
| 5 | **Flutter dart:ffi 集成验证** | 验证 Dart 侧能否加载 `.so` + 基础 roundtrip 测试 | ⚠️ FFI wiring 完成（http_client.dart + ws_client.dart 均已实现），需运行时加载 `.so` 验证 |

### P2 — 应该做的

| 序号 | 项目 | 工作内容 | 预估 |
|------|------|---------|------|
| 6 | **@catcher/web 增加 E2E 测试** | 用 playwright/puppeteer 跑 browser 侧验证 | 1h |
| 7 | **UniFFI 构建验证** | `cargo build` + 验证生成 Swift/Kotlin 绑定文件 | 20min |
| 8 | **retry.ts 额外修复** | `HttpClientConfig` 和 `RetryOptions` 类型不一致（retryIf vs minTimeout/maxTimeout）| 20min |
| 9 | **napi index.d.ts 与 TS @catcher/core 类型统一** | napi 用 Rust serde 字段名，TS 用 camelCase，考虑加映射层 | 30min |

### P3 — 锦上添花

| 序号 | 项目 | 工作内容 | 预估 |
|------|------|---------|------|
| 10 | proxy.ts corrupt/reorder/duplicate 损伤实现 | 接口已有，逻辑未写 | 30min |
| 11 | @catcher/web 发布到 npm | package.json 就绪，需 publish | 10min |
| 12 | Rust crate 发布到 crates.io | Cargo.toml 就绪，需 publish | 10min |

---

## 三、执行顺序

```
P1:  1(napi重编译) ✅  →  4(napi JS补全) ✅  →  2(web拦截器) ✅  →  3(web WS) ✅  →  5(Flutter验证) ⚠️
P2:  6(web测试) → 7(UniFFI构建) → 8(retry类型) → 9(napi类型映射)
P3:  10(proxy补全) → 11(npm发布) → 12(crates.io发布)
```

---

## 四、不做的事情

- ❌ Flutter WS client — dart:ffi 异步流推送复杂度高，暂无需求
- ❌ napi 层拦截器 — 跨 FFI 回调性能差，文档已说明
- ❌ @catcher/web DNS 缓存 — 浏览器不支持
- ❌ @catcher/web keepAlive — 浏览器自动管理

---

## 五、还缺的文档

| 文档 | 内容 |
|------|------|
| `docs/user-manual/rust.md` | Rust crate 使用指南（代码已有，文档没写） |
| `docs/user-manual/uniffi.md` | UniFFI 使用指南 |
| `docs/arch-ts/10-web.md` | @catcher/web 架构设计 |