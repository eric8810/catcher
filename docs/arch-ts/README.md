# catcher-ts 架构文档

> TypeScript 网络韧性库 — 三个独立 npm 包
> 代码位置：`packages/catcher-*-ts/`

---

## 包索引

| npm 包 | 目录 | 职责 | 运行时依赖 |
|--------|------|------|-----------|
| `@catcher/core` | `catcher-core-ts/` | 纯类型定义 | 无 |
| `@catcher/http` | `catcher-http-ts/` | HTTP 客户端 + Agent + 队列 | axios (peer), cockatiel, p-retry, p-queue, cacheable-lookup |
| `@catcher/ws` | `catcher-ws-ts/` | WebSocket 客户端 + msgpack 编解码 | ws (peer), msgpackr (optional peer) |

## 文档索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 00 | [`00-overview.md`](./00-overview.md) | 包概览、依赖关系、使用方式 |
| 02 | [`02-module-tree.md`](./02-module-tree.md) | 各包源码目录树 |
| 03 | [`03-types.md`](./03-types.md) | 类型定义（@catcher/core） |
| 04 | [`04-agent.md`](./04-agent.md) | 共享 Agent（@catcher/http） |
| 05 | [`05-http.md`](./05-http.md) | HTTP 客户端（@catcher/http） |
| 06 | [`06-ws.md`](./06-ws.md) | WebSocket 客户端 + msgpack 编解码（@catcher/ws） |
| 08 | [`08-queue.md`](./08-queue.md) | 优先级队列（@catcher/http） |
| 09 | [`09-interceptors.md`](./09-interceptors.md) | 拦截器系统 + Per-request Options 设计 |

> `07-codec.md` 已移除 — codec 不再是独立包，作为 `@catcher/ws` 的内置能力。

## 与 Rust 侧的对齐

| TS 包 | Rust crate |
|-------|-----------|
| `@catcher/core` | `catcher-core` |
| `@catcher/http` | `catcher-http` |
| `@catcher/ws` | `catcher-ws`（内置 codec） |