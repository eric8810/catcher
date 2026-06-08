# catcher 架构文档总索引

> v0.3.11 — 按协议分层，Rust / TS / napi-rs / Dart 四层统一拆分
> 代码位置：`packages/`

---

## 项目概述

catcher 是一个跨平台网络韧性库，覆盖 HTTP、WebSocket、Codec 三个协议域。四层统一拆分为独立包，按场景按需安装。

## 包全景

| 协议 | Rust | TS (纯 JS) | TS (napi 原生) | Dart |
|------|------|-----------|---------------|------|
| **types** | `catcher-core` ✅ Published | `@eric8810/catcher-core` ✅ Published | — | `catcher_core` ✅ Published |
| **HTTP** | `catcher-http` ✅ Published | `@eric8810/catcher-http` ✅ Published | `@eric8810/catcher-napi-http` ✅ Published | `catcher_core` ✅ Published |
| **SSE** | `catcher-http` 📐 设计中 | `@eric8810/catcher-http` 📐 设计中 | `@eric8810/catcher-napi-http` 📐 设计中 | 📐 设计中 |
| **WS** | `catcher-ws` ✅ Published | `@eric8810/catcher-ws` ✅ Published | `@eric8810/catcher-napi-ws` ✅ Published | `catcher_core` ✅ Published |
| **FFI** | `catcher-ffi` ✅ Published (cdylib umbrella) | — | — | (通过 catcher_ffi) |

> ✅ = 已实现  📐 = 设计中
> Codec 已合并到 WS — `catcher-ws` 内置 msgpack 编解码。
> SSE 基于 HTTP，归入 `catcher-http` / `catcher-web`。

## 架构文档

| 文档 | 内容 |
|------|------|
| [`arch-rs/`](./arch-rs/) | Rust workspace 架构（5 个 lib crate + 1 个 cdylib + 2 个 napi-rs + 1 个 uniffi） |
| [`arch-ts/`](./arch-ts/) | TypeScript 包架构（5 个 npm 包） |
| [`research/`](./research/) | 调研与决策分析 |

## 依赖关系图

```
                  catcher-core / @eric8810/catcher-core (零依赖)
                 /              \
                /                \
        catcher-http          catcher-ws
        @eric8810/catcher-http         @eric8810/catcher-ws (内置 codec)
        @eric8810/catcher-web          @eric8810/catcher-napi-http
        (browser)             @eric8810/catcher-napi-ws
                              (Node.js native)
             │                      │
        catcher-uniffi        dart:ffi (Flutter)
        (Swift + Kotlin)      catcher_core
```

> SSE 支持：`catcher-http`（Rust reqwest + tokio_stream）、`@eric8810/catcher-http`（Node fetch）、`@eric8810/catcher-web`（Browser fetch）均内置 SSE 能力。详见 [`arch-ts/10-sse.md`](../arch-ts/10-sse.md)。

- **Node.js** — TS (`@eric8810/catcher-http`) 或 native (`@eric8810/catcher-napi-http`)
- **Browser** — `@eric8810/catcher-web` (fetch)
- **Rust** — `catcher-http` / `catcher-ws` crate
- **Flutter** — `catcher_core` (dart:ffi)
- **Android + iOS** — `catcher-uniffi` (UniFFI → Swift + Kotlin)

## 按场景安装

```bash
# 场景 A: REST API (TS)
npm i @eric8810/catcher-http

# 场景 B: IM 实时通信 (TS + native)
npm i @eric8810/catcher-http @eric8810/catcher-ws @eric8810/catcher-napi-http @eric8810/catcher-napi-ws

# 场景 C: 浏览器
npm i @eric8810/catcher-web

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
| 04 | [`04-transport.md`](./04-transport.md) | HTTP + WS 传输层（reqwest / yawc） |
| 05 | [`05-resilience.md`](./05-resilience.md) | 重试、熔断、自适应超时 |
| 06 | [`06-scheduler.md`](./06-scheduler.md) | 优先级队列与并发调度 |
| 07 | [`07-codec.md`](./07-codec.md) | msgpack 编解码（已合并到 ws） |
| 08 | [`08-observability.md`](./08-observability.md) | 网络质量评估 + 指标收集 |
| 09 | [`09-ffi.md`](./09-ffi.md) | FFI 接口契约（C ABI / napi / dart:ffi） |
| 13 | [`13-dart-ffi.md`](./13-dart-ffi.md) | Dart FFI 绑定设计（dart:ffi ✅, flutter_rust_bridge ❌） |
| 14 | [`14-workspace.md`](./14-workspace.md) | v0.3 workspace 架构总览 |
| 15 | [`15-ffi-layering.md`](./15-ffi-layering.md) | FFI 分层策略：TS vs Rust 职责边界 + 待实现原生能力缺口 |
| 16 | [`16-napi-ts-wrapper.md`](./16-napi-ts-wrapper.md) | napi TS wrapper 架构 — 类型安全、事件类型化、tsup 构建 |
| 17 | [`17-dart-config-alignment.md`](./17-dart-config-alignment.md) | Dart FFI 配置类型对齐 — 默认值修正、SseReconnectConfig 重写、缺失字段补齐 |

## 阅读路径

- **新人**：本页 → [`14-workspace.md`](./14-workspace.md) → 按需深入各 crate
- **写代码**：在对应 `packages/catcher-*/` 下开发
- **了解决策**：[`research/`](../research/) 下的分析文档
- **理解分层**：[`15-ffi-layering.md`](./15-ffi-layering.md) — 什么放 Rust，什么放 TS
- **原生层缺口**：[`../issues/native-layer-capability-gaps.md`](../issues/native-layer-capability-gaps.md) — TS 已有但 Rust 原生层未对等覆盖的能力
