# Catcher 使用手册

> 跨平台网络韧性库的完整开发者文档

---

## 目录结构

### 平台快速上手

| 文档 | 说明 |
|------|------|
| [`nodejs.md`](./nodejs.md) | Node.js / Electron — TS 版 + napi 原生版双轨指南 |
| [`flutter.md`](./flutter.md) | Flutter — dart:ffi 绑定，HTTP / WebSocket / 编解码 |
| [`web.md`](./web.md) | Web 浏览器 — fetch-based，HTTP / SSE / WebSocket |
| [`rust.md`](./rust.md) | Rust — `catcher-http` / `catcher-ws` / `catcher-core` crate 使用 |

### 概念深度指南

| 文档 | 说明 |
|------|------|
| [`resilience.md`](./resilience.md) | 韧性策略 — retry（退避、可重试条件）、circuit breaker（三态机、参数调优）、超时层级、自适应超时 |
| [`sse.md`](./sse.md) | SSE 完整指南 — 一次性流 / 长连接推送、AI 对话集成、Last-Event-ID 断点续传、SSE Router 事件路由 |
| [`websocket.md`](./websocket.md) | WebSocket 深度指南 — 多端点竞速、per-message-deflate、自动重连、msgpack 编解码、心跳 RTT |
| [`websocket-permessage-deflate-server-integration.md`](./websocket-permessage-deflate-server-integration.md) | WebSocket 服务端对接说明 — `permessage-deflate` 握手、服务端适配要点、验收建议 |
| [`error-handling.md`](./error-handling.md) | 错误处理与诊断 — CatcherError 类型体系、错误分类、超时 vs 连接失败 vs 业务错误 |
| [`migration.md`](./migration.md) | 迁移指南 — 从 axios / fetch / 原生 WebSocket 迁移到 catcher 的逐 API 对照表 |

### 常见问题

| 文档 | 说明 |
|------|------|
| [`faq.md`](./faq.md) | CORS、代理配置、Electron 渲染进程、Flutter 产物打包、napi 平台兼容性等 |

### API Reference

按包列出所有公开 API 的签名、参数说明、默认值、返回值、用法示例。

| 文档 | 覆盖包 |
|------|--------|
| [`api/ts-http.md`](./api/ts-http.md) | `@eric8810/catcher-http` — createHttpClient, interceptors, SSE, queue, agent |
| [`api/ts-ws.md`](./api/ts-ws.md) | `@eric8810/catcher-ws` — createResilientWS, pack/unpack, raceEndpoints |
| [`api/ts-web.md`](./api/ts-web.md) | `@eric8810/catcher-web` — createWebClient, createWebSocketClient, SSE |
| [`api/dart.md`](./api/dart.md) | `catcher_core` (pub.dev) — CatcherHttpClient, CatcherWsClient, codec, quality |
| [`api/rust-http.md`](./api/rust-http.md) | `catcher-http` crate — HttpTransport, CircuitBreaker, PriorityRequestQueue, SseClient |
| [`api/rust-ws.md`](./api/rust-ws.md) | `catcher-ws` crate — WsTransport, HeartbeatManager, ReconnectManager, codec |
| [`api/napi.md`](./api/napi.md) | `@eric8810/catcher-napi-http` / `catcher-napi-ws` — JSON config schema |

---

## 平台支持状态

| 平台 | 状态 | 方案 |
|------|------|------|
| **Node.js (native)** | ✅ 可用 | `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws` (Rust via napi-rs) |
| **Node.js (TS)** | ✅ 可用 | `@eric8810/catcher-http` / `@eric8810/catcher-ws` (纯 TS，API 更丰富) |
| **Electron** | ✅ 同 Node.js | napi 或 TS 包均可 |
| **Rust** | ✅ 已实现 | `catcher-http` + `catcher-ws` + `catcher-core` crate |
| **Web (Browser)** | ✅ 已发布 | `@eric8810/catcher-web` — fetch-based, 纯 TS |
| **Android + iOS** | ✅ 已发布 | UniFFI → Swift + Kotlin |
| **Flutter** | ✅ 已发布 | `catcher_core` (pub.dev) — dart:ffi → C ABI |

---

## 快速选择

```
                      需要网络韧性？
                           │
      ┌────────────────────┼────────────────────┐
      ▼                    ▼                    ▼
  Node.js/Electron     Rust / 移动端         浏览器
      │                    │                    │
 ┌────┴────┐          ┌────┴────┐          @eric8810/catcher-web
 ▼         ▼          ▼         ▼          (fetch)
napi     TS版      Rust crate  Flutter
native   (API更全)  (已实现)    dart:ffi
```

---

## 包关系

```
catcher-core (Rust)              @eric8810/catcher-core (TS)
     │                                │
 ┌───┴───┐                        ┌───┴───┐
 ▼       ▼                        ▼       ▼
catcher  catcher           @eric8810  @eric8810
-http    -ws               /catcher-  /catcher-
 │  │     │  │              http       ws
 │  │     │  │               (TS版)    (TS版)
 │  │     │  │
 │  └──napi-rs──┐   ┌──napi-rs──┘
 │              ▼   ▼
 │        @eric8810/catcher-napi-http
 │        @eric8810/catcher-napi-ws
 │         (Node.js native)
 │
 ├── UniFFI → Swift + Kotlin (Android/iOS)
 └── C ABI  → dart:ffi (Flutter)
```
