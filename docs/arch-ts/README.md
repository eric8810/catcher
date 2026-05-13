# catcher-ts 架构文档

> TypeScript 网络韧性库 — 四个独立 npm 包（+ 一个 Rust native 绑定包）
> 代码位置：`packages/catcher-*-ts/` + `packages/catcher-web/`

---

## 包索引

| npm 包 | 目录 | 职责 | 运行时依赖 |
|--------|------|------|-----------|
| `@eric8810/catcher-core` | `catcher-core-ts/` | 纯类型定义 | 无 |
| `@eric8810/catcher-http` | `catcher-http-ts/` | HTTP 客户端 + Agent + 队列 + **SSE** | axios (peer), cockatiel, p-retry, p-queue, cacheable-lookup |
| `@eric8810/catcher-ws` | `catcher-ws-ts/` | WebSocket 客户端 + msgpack 编解码 | ws (peer), msgpackr (optional peer) |
| `@eric8810/catcher-web` | `catcher-web/` | 浏览器 HTTP 客户端（fetch-based）+ **SSE** | cockatiel, p-retry, p-queue |

## 文档索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 00 | [`00-overview.md`](./00-overview.md) | 包概览、依赖关系、使用方式 |
| 02 | [`02-module-tree.md`](./02-module-tree.md) | 各包源码目录树 |
| 03 | [`03-types.md`](./03-types.md) | 类型定义（@eric8810/catcher-core） |
| 04 | [`04-agent.md`](./04-agent.md) | 共享 Agent（@eric8810/catcher-http） |
| 05 | [`05-http.md`](./05-http.md) | HTTP 客户端（@eric8810/catcher-http） |
| 06 | [`06-ws.md`](./06-ws.md) | WebSocket 客户端 + msgpack 编解码（@eric8810/catcher-ws） |
| 08 | [`08-queue.md`](./08-queue.md) | 优先级队列（@eric8810/catcher-http） |
| 09 | [`09-interceptors.md`](./09-interceptors.md) | 拦截器系统 + Per-request Options 设计 |
| 10 | [`10-sse.md`](./10-sse.md) | **Server-Sent Events 客户端（AI 流式响应）** |

> `07-codec.md` 已移除 — codec 不再是独立包，作为 `@eric8810/catcher-ws` 的内置能力。

## 与 Rust 侧的对齐

| TS 包 | Rust crate |
|-------|-----------|
| `@eric8810/catcher-core` | `catcher-core` |
| `@eric8810/catcher-http` | `catcher-http` / `catcher-napi-http` (napi-rs) |
| `@eric8810/catcher-ws` | `catcher-ws` / `catcher-napi-ws` (napi-rs) |
| `@eric8810/catcher-web` | — (纯 TS, fetch-based) |

> **SSE 模块**仅存在于 TS 层（`catcher-http` + `catcher-web`），不涉及 Rust / FFI / napi。SSE 基于 `fetch` + `ReadableStream`，是纯 TypeScript 实现。