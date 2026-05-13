# Catcher 使用手册

> 按平台选择使用方式

---

## 平台支持状态

| 平台 | 状态 | 方案 |
|------|------|------|
| **Node.js (native)** | ✅ 可用 | `@catcher/napi-http` / `@catcher/napi-ws` (Rust via napi-rs) |
| **Node.js (TS)** | ✅ 可用 | `@catcher/http` / `@catcher/ws` (纯 TS，API 更丰富) |
| **Electron** | ✅ 同 Node.js | napi 或 TS 包均可 |
| **Rust** | ✅ 已实现 | `catcher-http` + `catcher-ws` + `catcher-core` crate |
| **Web (Browser)** | ⚠️ 缺失 | `@catcher/web` — 唯一需要 TS 新建的平台 |
| **Android + iOS** | 📋 规划 | UniFFI → Swift + Kotlin（Rust 核心已就绪） |
| **Flutter** | 📋 规划 | dart:ffi → 现有 C ABI（Rust 核心已就绪） |

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
 ┌────┴────┐          ┌────┴────┐          @catcher/web
 ▼         ▼          ▼         ▼          (缺失，待建)
napi     TS版      Rust crate  Flutter
native   (API更全)  (已实现)    dart:ffi
(已编译)                     (C ABI 已就绪)
```

---

## 包关系

```
catcher-core (Rust)              @catcher/core (TS)
     │                                │
 ┌───┴───┐                        ┌───┴───┐
 ▼       ▼                        ▼       ▼
catcher  catcher             @catcher  @catcher
-http    -ws                 /http     /ws
 │  │     │  │               (TS版)    (TS版)
 │  │     │  │
 │  └──napi-rs──┐   ┌──napi-rs──┘
 │              ▼   ▼
 │        @catcher/napi-http
 │        @catcher/napi-ws
 │         (Node.js native)
 │
 ├── UniFFI → Swift + Kotlin (Android/iOS, 规划)
 └── C ABI  → dart:ffi (Flutter, 规划)
```

---

## 各平台差异

| | Node.js native | Node.js TS | Web | Rust/移动端 |
|--|:--:|:--:|:--:|:--:|
| 网络层 | reqwest (Rust) | axios | fetch | reqwest (Rust) |
| 韧性层 | catcher-rs (Rust) | p-retry/cockatiel (TS) | p-retry/cockatiel (TS) | catcher-rs (Rust) |
| 编解码 | msgpack (Rust) | msgpackr (TS) | msgpackr (TS) | msgpack (Rust) |
| 连接池 | ✅ Rust pool | ✅ Agent keepAlive | ❌ 浏览器管理 | ✅ Rust pool |
| 拦截器 | ❌ (待暴露) | ✅ 完整 | ✅ (待建) | ❌ |
| 状态 | ✅ | ✅ | ⚠️ | 📋 (Rust ✅, 绑定 📋) |

> 详细调研见 [`research/platform-support-analysis.md`](../research/platform-support-analysis.md)
