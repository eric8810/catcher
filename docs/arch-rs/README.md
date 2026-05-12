# catcher 架构文档总索引

> v0.2.0 — 按协议分层，Rust / TS / napi-rs / Dart 四层统一拆分
> 代码位置：`packages/`

---

## 项目概述

catcher 是一个跨平台网络韧性库，覆盖 HTTP、WebSocket、Codec 三个协议域。四层统一拆分为独立包，按场景按需安装。

## 包全景

| 协议 | Rust | TS (纯 JS) | TS (napi 原生) | Dart |
|------|------|-----------|---------------|------|
| **types** | `catcher-core` | `@catcher/core` | — | `catcher_core` |
| **HTTP** | `catcher-http` | `@catcher/http` | `@catcher/napi-http` | `catcher_core` |
| **WS** | `catcher-ws` | `@catcher/ws` | `@catcher/napi-ws` | `catcher_core` |

> Codec 已合并到 WS — `catcher-ws` 内置 msgpack 编解码，`@catcher/ws` 通过 optional peer `msgpackr` 支持。

## 架构文档

| 文档 | 内容 |
|------|------|
| [`arch-rs/`](./arch-rs/) | Rust workspace 架构（4 个 lib crate + 3 个 napi-rs crate） |
| [`arch-ts/`](./arch-ts/) | TypeScript 包架构（4 个 npm 包） |
| [`research/`](./research/) | 调研与决策分析 |

## 依赖关系图

```
                  catcher-core / @catcher/core (零依赖)
                 /              \
                /                \
        catcher-http          catcher-ws
        @catcher/http         @catcher/ws (内置 codec)
```

- **无相互依赖** — HTTP、WS 二者互不依赖
- **core 零依赖** — 仅含纯类型/错误定义
- **codec 已合并到 ws** — msgpack 编解码是 `@catcher/ws` / `catcher-ws` 的内置能力
- **无 umbrella** — 无中间聚合层，调用方按需直接引用

## 按场景安装

```bash
# 场景 A: REST API (TS)
npm i @catcher/http

# 场景 B: IM 实时通信 (TS + native)
npm i @catcher/http @catcher/ws @catcher/napi-http @catcher/napi-ws

# 场景 C: 文件上传 (Rust + TS)
cargo add catcher-http
npm i @catcher/http @catcher/napi-http

# 场景 D: Flutter 全功能
# pubspec.yaml: catcher_core
```

## 文档索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 01 | [`01-cargo.md`](./01-cargo.md) | Cargo workspace 与 crate 间依赖 |
| 02 | [`02-module-tree.md`](./02-module-tree.md) | 各 crate 源码目录树 |
| 03 | [`03-types.md`](./03-types.md) | 类型定义（catcher-core） |
| 04 | [`04-transport.md`](./04-transport.md) | HTTP + WS 传输层（reqwest / tokio-tungstenite） |
| 05 | [`05-resilience.md`](./05-resilience.md) | 重试、熔断、自适应超时 |
| 06 | [`06-scheduler.md`](./06-scheduler.md) | 优先级队列与并发调度 |
| 07 | [`07-codec.md`](./07-codec.md) | msgpack 编解码（已合并到 ws） |
| 08 | [`08-observability.md`](./08-observability.md) | 网络质量评估 + 指标收集 |
| 09 | [`09-ffi.md`](./09-ffi.md) | FFI 接口契约（C ABI / napi / flutter_rust_bridge） |
| 10 | [`10-error-handling.md`](./10-error-handling.md) | 错误类型与传播 |
| 11 | [`11-testing.md`](./11-testing.md) | 测试策略 |
| 12 | [`12-state-machines.md`](./12-state-machines.md) | 重连/熔断/心跳状态机 |
| 13 | [`13-dart-ffi.md`](./13-dart-ffi.md) | Dart FFI 绑定设计 |
| 14 | [`14-workspace.md`](./14-workspace.md) | v0.2 workspace 架构总览 |
| 15 | [`15-ffi-layering.md`](./15-ffi-layering.md) | FFI 分层策略：TS vs Rust 职责边界 |

## 阅读路径

- **新人**：本页 → [`14-workspace.md`](./14-workspace.md) → 按需深入各 crate
- **写代码**：在对应 `packages/catcher-*/` 下开发
- **了解决策**：[`research/`](../research/) 下的分析文档
- **理解分层**：[`15-ffi-layering.md`](./15-ffi-layering.md) — 什么放 Rust，什么放 TS
