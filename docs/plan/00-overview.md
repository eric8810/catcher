# 00 — 开发方案总览

> 基于 `docs/arch-rs/` 完整架构文档
> 目标：catcher-rs (Rust 核心) + napi-rs 绑定 → 复用 TS e2e 测试验证

---

## 开发策略

### 核心原则：逐层推进，每层可独立测试

```
Phase 1: types + error + codec     ← 纯函数，零 I/O，风险最低
    ↓
Phase 2: transport (HTTP + WS)     ← 真实 I/O，可用 wiremock 集成测试
    ↓
Phase 3: resilience                ← 纯状态机 + 策略，依赖 transport trait
    ↓
Phase 4: scheduler + observability ← 依赖 resilience + transport
    ↓
Phase 5: FFI bindings              ← C ABI → napi-rs → 复用 TS e2e 测试
```

### 为什么这个顺序

| 决策 | 理由 |
|------|------|
| Codec 最先 | 纯函数无副作用，验证 crate 结构、CI、测试框架全部就绪 |
| Transport 其次 | 先让 HTTP/WS "能跑"，再叠加韧性策略 |
| Resilience 第三 | 依赖 transport trait，但自身是纯状态机，单测覆盖充分 |
| Scheduler 第四 | 依赖上层组合，Semaphore + mpsc 调度 |
| FFI 最后 | 前面全部就绪后，暴露 FFI，用 TS e2e 做最终验证 |

---

## 5 个 Phase 总览

| Phase | 内容 | 源文件数 | 关键依赖 | 可测试性 |
|-------|------|---------|---------|---------|
| 1 — Foundation | error.rs, config.rs, types/, codec/ | 9 | serde, rmp-serde, rmpv | 纯单元测试 |
| 2 — Transport | transport/http_client, ws_client, tls, dns | 4 | reqwest, stream-tungstenite | wiremock 集成测试 |
| 3 — Resilience | resilience/retry, circuit_breaker, backoff, timeout | 4 | backon, circuitbreaker-rs | 纯单元测试 (mock transport trait) |
| 4 — Scheduler + Obs | scheduler/, observability/ | 4 | tokio::sync | 单元 + 集成 |
| 5 — FFI | ffi/ (5 files) + napi 绑定包 + FRB 绑定包 | 5+ | napi-rs, flutter_rust_bridge | **复用 TS e2e 测试** |

---

## TS e2e 测试复用策略

### 核心洞察

现有 `packages/catcher-ts/test/` 目录包含完整的端到端测试基础设施：

| 组件 | 文件 | 作用 | 复用方式 |
|------|------|------|---------|
| NetworkProxy | `test/network/proxy.ts` | 模拟延迟/丢包/限宽/断连 | **直接复用** — 代理 Rust 核心的 HTTP/WS 流量 |
| 网络预设 | `test/network/presets.ts` | 7 种网络环境 (good→metro) | **直接复用** — 相同的条件跑 Rust 验证 |
| HTTP 测试服务器 | `test/servers/http-server.ts` | 模拟 IM API Gateway | **直接复用** — 相同的 endpoint 和响应 |
| WS 测试服务器 | `test/servers/ws-server.ts` | echo + heartbeat | **直接复用** — 相同的 WS 行为 |
| Harness | `test/harness.ts` | 并发对比 + 指标计算 | **直接复用** — 替换 catcherFn 为 Rust 实现 |
| ComparisonReporter | `test/reporters/comparison-reporter.ts` | 生成对比报告 | **直接复用** |
| 8 个 E2E 场景 | `test/e2e/scenarios.test.ts` | S1-S8 业务场景 | **改写适配** — vanilla (axios/ws) vs Rust (napi-rs) |
| Chaos 测试 | `test/chaos/chaos.test.ts` | 长时间随机网络波动 | **改写适配** — 只测 Rust 核心 |

### 对比模式

```
现有:  vanilla (axios)  vs  catcher (TS 实现)
          ↓                        ↓
改写:  vanilla (axios)  vs  catcher (Rust via napi-rs)
```

Rust 核心通过 napi-rs 编译为 `.node` addon，暴露相同的 `HttpClient` / `WsClient` / `pack` / `unpack` 接口。TS 测试文件只需把 `import { createHttpClient } from '../../src/http/client.js'` 替换为 `import { HttpClient } from 'catcher-rs'`，其余测试基础设施（proxy、server、harness）完全不变。

### Phase 5 的验证闭环

```
                    ┌────────────────────────────┐
                    │   Rust core (catcher-rs)    │
                    │   src/{transport, resilience,│
                    │        scheduler, codec}    │
                    └──────────┬─────────────────┘
                               │ C ABI (src/ffi/)
                    ┌──────────▼─────────────────┐
                    │   napi-rs 绑定层             │
                    │   catcher-rs-napi/           │
                    └──────────┬─────────────────┘
                               │ .node addon
                    ┌──────────▼─────────────────┐
                    │   TS e2e test suite         │
                    │   packages/catcher-ts/test/ │
                    │   (proxy + server + harness) │
                    └────────────────────────────┘
```

---

## 工作量估算

| Phase | Rust 核心 | 测试 | napi 绑定 | 合计 |
|-------|----------|------|----------|------|
| 1 — Foundation | 3d | 1d | — | **4d** |
| 2 — Transport | 5d | 2d | — | **7d** |
| 3 — Resilience | 4d | 2d | — | **6d** |
| 4 — Scheduler + Obs | 3d | 2d | — | **5d** |
| 5 — FFI + e2e 验证 | 3d | 3d (复用 TS) | 3d | **9d** |
| CI/CD + 文档 | 2d | — | 1d | **3d** |
| **总计** | **20d** | **10d** | **4d** | **~34 人天** |

> 1 人全职 ~7 周；2 人并行可压缩到 ~4-5 周。

---

## 文档索引

| 编号 | 文件 | 内容 |
|------|------|------|
| 00 | `00-overview.md` | 开发总览、阶段划分、测试复用策略 |
| 01 | `01-scaffold.md` | Cargo 项目脚手架、CI 配置、目录创建 |
| 02 | `02-phase1-types-codec.md` | Phase 1: error + config + types + codec |
| 03 | `03-phase2-transport.md` | Phase 2: HTTP transport + WS transport + TLS + DNS |
| 04 | `04-phase3-resilience.md` | Phase 3: retry + circuit breaker + backoff + adaptive timeout |
| 05 | `05-phase4-scheduler-observability.md` | Phase 4: priority queue + concurrency + network quality + metrics |
| 06 | `06-phase5-ffi.md` | Phase 5: C ABI + napi-rs + flutter_rust_bridge |
| 07 | `07-test-reuse.md` | TS e2e 测试复用详细方案 |
