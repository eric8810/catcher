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
                 /         |          \
                /          |           \
        catcher-http  catcher-ws  catcher-codec
        @catcher/http @catcher/ws @catcher/codec
```

- **无相互依赖** — HTTP、WS、Codec 三者互不依赖
- **core 零依赖** — 仅含纯类型/错误定义
- **无 umbrella** — 无中间聚合层，调用方按需直接引用

## 按场景安装

```bash
# 场景 A: REST API (TS)
npm i @catcher/http

# 场景 B: IM 实时通信 (TS + native)
npm i @catcher/http @catcher/ws @catcher/codec @catcher/napi-http @catcher/napi-ws

# 场景 C: 文件上传 (Rust + TS)
cargo add catcher-http
npm i @catcher/http @catcher/napi-http

# 场景 D: Flutter 全功能
# pubspec.yaml: catcher_core
```

## 阅读路径

- **新人**：本页 → [`arch-rs/14-workspace.md`](./arch-rs/14-workspace.md) → 按需深入各 crate
- **写代码**：在对应 `packages/catcher-*/` 下开发
- **了解决策**：[`research/`](./research/) 下的分析文档
