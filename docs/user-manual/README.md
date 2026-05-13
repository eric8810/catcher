# Catcher 使用手册

> 按平台选择使用方式

---

## 平台支持状态

| 平台 | 状态 | 方案 |
|------|------|------|
| **Node.js (native)** | ✅ 可用 | `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws` (Rust via napi-rs) |
| **Node.js (TS)** | ✅ 可用 | `@eric8810/catcher-http` / `@eric8810/catcher-ws` (纯 TS，API 更丰富) |
| **Electron** | ✅ 同 Node.js | napi 或 TS 包均可 |
| **Rust** | ✅ 已实现 | `catcher-http` + `catcher-ws` + `catcher-core` crate |
| **Web (Browser)** | ✅ 已发布 | `@eric8810/catcher-web` — fetch-based, 纯 TS |
| **Android + iOS** | ⚠️ WIP | UniFFI → Swift + Kotlin |
| **Flutter** | ✅ 已发布 | `catcher_core` (pub.dev) — dart:ffi → C ABI |

---

## 手册索引

| 文档 | 内容 |
|------|------|
| [`nodejs.md`](./nodejs.md) | Node.js / Electron（TS + napi 双轨） |
| [`flutter.md`](./flutter.md) | Flutter (dart:ffi) |
| [`web.md`](./web.md) | Web 浏览器 |

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

---

## 各平台差异

| | Node.js native | Node.js TS | Web | Rust/移动端 |
|--|:--:|:--:|:--:|:--:|
| 网络层 | reqwest (Rust) | axios | fetch | reqwest (Rust) |
| 韧性层 | catcher-rs (Rust) | p-retry/cockatiel (TS) | p-retry/cockatiel (TS) | catcher-rs (Rust) |
| 编解码 | msgpack (Rust) | msgpackr (TS) | msgpackr (TS) | msgpack (Rust) |
| 连接池 | ✅ Rust pool | ✅ Agent keepAlive | ❌ 浏览器管理 | ✅ Rust pool |
| 拦截器 | ❌ (待暴露) | ✅ 完整 | ⏳ 基础 (stub) | ❌ |
| 状态 | ✅ | ✅ | ✅ | ✅ (Rust ✅, 绑定 ✅) |

> 详细调研见 [`research/platform-support-analysis.md`](../research/platform-support-analysis.md)
