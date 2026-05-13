# catcher 架构文档总索引

> v0.2.0 — 按协议分层，Rust / TS / napi-rs / Dart 四层统一拆分
> 代码位置：`packages/`

---

## 项目概述

catcher 是一个跨平台网络韧性库，覆盖 HTTP、WebSocket、Codec 三个协议域。四层统一拆分为独立包，按场景按需安装。

## 包全景

| 协议 | Rust | TS (纯 JS) | TS (napi 原生) | Dart |
|------|------|-----------|---------------|------|
| **types** | `catcher-core` ✅ | `@eric8810/core` ✅ | — | `catcher_core` ✅ |
| **HTTP** | `catcher-http` ✅ | `@eric8810/http` ✅ | `@eric8810/napi-http` ✅ | `catcher_core` ✅ |
| **WS** | `catcher-ws` ✅ | `@eric8810/ws` ✅ | `@eric8810/napi-ws` ✅ | `catcher_core` ✅ |

> ✅ = 已实现  
> Codec 已合并到 WS — `catcher-ws` 内置 msgpack 编解码。

## 架构文档

| 文档 | 内容 |
|------|------|
| [`arch-rs/`](./arch-rs/) | Rust workspace 架构（4 个 lib crate + 2 个 napi-rs + 1 个 uniffi） |
| [`arch-ts/`](./arch-ts/) | TypeScript 包架构（5 个 npm 包） |
| [`research/`](./research/) | 调研与决策分析 |

## 依赖关系图

```
                  catcher-core / @eric8810/core (零依赖)
                 /              \
                /                \
        catcher-http          catcher-ws
        @eric8810/http         @eric8810/ws (内置 codec)
        @eric8810/web          @eric8810/napi-http
        (browser)             @eric8810/napi-ws
                              (Node.js native)
             │                      │
        catcher-uniffi        dart:ffi (Flutter)
        (Swift + Kotlin)      catcher_core
```

- **Node.js** — TS (`@eric8810/http`) 或 native (`@eric8810/napi-http`)
- **Browser** — `@eric8810/web` (fetch)
- **Rust** — `catcher-http` / `catcher-ws` crate
- **Flutter** — `catcher_core` (dart:ffi)
- **Android + iOS** — `catcher-uniffi` (UniFFI → Swift + Kotlin)

## 按场景安装

```bash
# 场景 A: REST API (TS)
npm i @eric8810/http

# 场景 B: IM 实时通信 (TS + native)
npm i @eric8810/http @eric8810/ws @eric8810/napi-http @eric8810/napi-ws

# 场景 C: 浏览器
npm i @eric8810/web

# 场景 D: Rust
cargo add catcher-http catcher-ws

# 场景 E: Flutter
flutter pub add catcher_core
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
| 09 | [`09-ffi.md`](./09-ffi.md) | FFI 接口契约（C ABI / napi / dart:ffi） |
| 13 | [`13-dart-ffi.md`](./13-dart-ffi.md) | Dart FFI 绑定设计（dart:ffi ✅, flutter_rust_bridge ❌） |
| 14 | [`14-workspace.md`](./14-workspace.md) | v0.2 workspace 架构总览 |
| 15 | [`15-ffi-layering.md`](./15-ffi-layering.md) | FFI 分层策略：TS vs Rust 职责边界 |

## 阅读路径

- **新人**：本页 → [`14-workspace.md`](./14-workspace.md) → 按需深入各 crate
- **写代码**：在对应 `packages/catcher-*/` 下开发
- **了解决策**：[`research/`](../research/) 下的分析文档
- **理解分层**：[`15-ffi-layering.md`](./15-ffi-layering.md) — 什么放 Rust，什么放 TS
