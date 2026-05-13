# TS / Dart 包拆分策略

> 对照 Rust 侧已完成的 crate 拆分，分析 TS 和 Dart 侧是否也需要拆包。
> 核心依据与前文一致：**使用场景和生命周期不同，不应强行捆在一起。**

---

## 一、当前 TS 包结构

`packages/catcher-ts/` — 单 npm 包 `catcher`，通过 sub-path exports 暴露：

```json
{
  "exports": {
    ".":        "./dist/index.mjs",
    "./http":   "./dist/http/index.mjs",
    "./ws":     "./dist/ws/index.mjs",
    "./codec":  "./dist/codec/index.mjs"
  }
}
```

sub-path exports 解决了**代码引用**层面的隔离，但**没有解决包依赖和心智模型层面的隔离**：

- `npm i catcher` → 装 7 个依赖（axios, ws, msgpackr, cockatiel, p-retry, p-queue, cacheable-lookup）
- 只用 HTTP 的用户也要承受 `msgpackr`（WS codec 专用）和 `ws`（WebSocket）的安装
- 所有模块共享同一版本号，改 WS 的行为也得跟着发大版本

---

## 二、三个模块的生命周期差异（与 Rust 侧完全一致）

| 维度 | HTTP Client | WebSocket | Codec |
|------|-------------|-----------|-------|
| **连接模型** | 短连接 RPC | 长连接流 | 纯函数（无连接） |
| **状态管理** | 无状态 | 多状态机（Disconnected/Connecting/Connected/Reconnecting） | 无状态 |
| **错误处理** | 请求级错误（4xx/5xx/timeout） | 连接级错误（断开/重连/心跳超时） | 数据级错误（Encode/Decode） |
| **资源占用** | 低，连接池复用 | 高，持久连接 + heartbeat timer | 极低 |
| **用户场景** | REST API、文件上传 | 实时消息推送 | 二进制序列化 |
| **谁在用** | 所有应用 | IM 类应用 | IM 类应用（WS 消息编码） |

**同一个用户不会同时使用三者**：做 REST API 的不用 WS，做 IM 的才需要 WS + codec，做文件上传的只需要 HTTP。

---

## 三、TS 侧：应该拆

### 目标结构

```
packages/
├── catcher-core-ts/       # @eric8810/core    — 纯 types，零运行时依赖
│   exports: HttpClientConfig, RetryConfig, CircuitBreakerConfig, WsClientConfig, ...
│   deps: (none)
│
├── catcher-http-ts/       # @eric8810/http    — HTTP 客户端
│   exports: createHttpClient, createRetryWrapper, createSharedAgent, createPriorityQueue
│   deps: @eric8810/core, cockatiel, p-retry, p-queue, cacheable-lookup
│   peerDeps: axios
│
├── catcher-ws-ts/         # @eric8810/ws      — WebSocket 客户端
│   exports: createResilientWS, createReconnectStrategy, raceEndpoints
│   deps: @eric8810/core
│   peerDeps: ws
│
└── catcher-codec-ts/      # @eric8810/codec   — 编解码
    exports: pack, unpack, isBinary, decodeWSMessage
    deps: msgpackr
```

### 按场景安装

```bash
# 场景 A: 纯 HTTP API
npm i @eric8810/http

# 场景 B: IM 实时通信
npm i @eric8810/http @eric8810/ws @eric8810/codec

# 场景 C: 文件上传
npm i @eric8810/http
```

### 不拆的代价

- `msgpackr`（WS codec 专用，但 HTTP 用户也要装）
- `ws` 包需要 node-gyp 原生编译，HTTP 用户白白承受
- HTTP 和 WS 的版本绑死，WS 修个重连 bug 也得跟 HTTP 一起发版
- 新人看文档要同时理解 HTTP 和 WS 两套 API

---

## 四、Dart 侧

当前 `catcher_core` 是单 pub 包。

### 应该拆，但优先级放后

| 因素 | 说明 |
|------|------|
| **Rust 层已拆** | `catcher-http` / `catcher-ws` / `catcher-codec` 三个独立 crate，各自编译 `.so` |
| **Dart 层心智模型应一致** | 用户不应在 Dart 侧看到 `CatcherHttpClient` 和 `CatcherWsClient` 混在一个包里 |
| **阻止条件** | Flutter `native_assets` 系统的跨包链接支持还在演进中 |

### 推荐目标结构

```
pub.dev:
  catcher_http   — HttpClient (deps: catcher_core FFI → catcher-http .so)
  catcher_ws     — WsClient (deps: catcher_core FFI → catcher-ws .so)
  catcher_codec  — Codec (deps: catcher_core FFI → catcher-codec .so)
```

### 当前状态：单包 + 内部按模块隔离

```dart
// 同一个 catcher_core 包内，但 API 已分层
import 'package:catcher_core/catcher_core.dart';
final http = CatcherHttpClient(config);
final ws = CatcherWsClient(config);
```

短期可接受，长期应与 Rust/TS 对齐。

---

## 五、四层一致性总览

| 层 | 当前 | 目标 |
|----|------|------|
| **Rust** | ✅ `catcher-core` + `catcher-http` + `catcher-ws` + `catcher-codec` | 已完成 |
| **TS** | ⚠️ 单包 `catcher`，sub-path exports | `@eric8810/core` + `@eric8810/http` + `@eric8810/ws` + `@eric8810/codec` |
| **Dart** | ⚠️ 单包 `catcher_core` | `catcher_http` + `catcher_ws` + `catcher_codec` |
| **napi-rs** | ✅ `@eric8810/napi-http` + `@eric8810/napi-ws` + `@eric8810/napi-codec` | 已完成 |

---

## 六、TS 拆分优先级

| 优先级 | 动作 | 理由 |
|--------|------|------|
| **P0** | 创建 `@eric8810/core` | 纯 types，零运行时依赖，所有子包的基础 |
| **P0** | 创建 `@eric8810/http` + `@eric8810/ws` + `@eric8810/codec` | 三个子包，各自仅带必要依赖 |
| **P1** | 保留 `catcher` umbrella 兼容 | 存量用户 `import from 'catcher'` 不 break |
| **P2** | 移除 umbrella | catacher 成熟后，用户直接用子包 |
