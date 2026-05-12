# 测试架构

> catcher 网络韧性测试框架 — 从损伤模拟到场景验证的完整体系

---

## 核心组件

```
┌──────────────────────────────────────────────────┐
│  测试场景 (scenarios / benchmarks)                │
│  S1-S16 场景 + 吞吐基准                          │
├──────────────────────────────────────────────────┤
│  测试 Harness (harness.ts)                       │
│  并发对比、指标采集、统计汇总                      │
├──────────────────────────────────────────────────┤
│  网络 Profile (presets.ts)                       │
│  Wi-Fi / 4G / 3G / 2G / GPRS / 卫星 / 地铁       │
├──────────────────────────────────────────────────┤
│  网络损伤代理 (proxy.ts)                          │
│  延迟、抖动、丢包、突发、黑洞、带宽               │
├──────────────────────────────────────────────────┤
│  测试服务 (servers/)                              │
│  HTTP API Gateway + WebSocket Server             │
└──────────────────────────────────────────────────┘
```

## 目录

```
docs/test/
├── README.md           ← 本文件
├── 00-overview.md      ← 测试框架概览 + 设计哲学
├── 01-proxy.md         ← 网络损伤代理设计（损伤模型）
├── 02-profiles.md      ← 网络 Profile 体系（对标标准）
└── 03-scenarios.md     ← 测试场景体系（S1-S16）

packages/test/
├── harness.ts                 ← 并发对比 Harness
├── benchmark/
│   └── throughput.test.ts     ← 高并发吞吐基准
├── integration/
│   ├── http.test.ts           ← HTTP 集成测试
│   ├── ws.test.ts             ← WebSocket 集成测试
│   ├── dns.test.ts            ← DNS 缓存测试
│   └── napi.test.ts           ← napi 绑定测试
├── e2e/
│   ├── scenarios.test.ts      ← S1-S8 端到端场景
│   └── rust-vs-vanilla.test.ts ← Rust vs vanilla 对比
├── chaos/
│   ├── chaos.test.ts          ← 混沌测试
│   └── extreme-scenarios.test.ts  ← S9-S16 极端场景（新增）
├── network/
│   ├── proxy.ts               ← 网络损伤代理
│   └── presets.ts             ← 网络 Profile 预设
├── servers/
│   ├── http-server.ts         ← HTTP 测试服务
│   └── ws-server.ts           ← WebSocket 测试服务
└── reporters/
    └── comparison-reporter.ts ← vanilla vs catcher 对比报表
```

## 设计哲学

1. **与真实网络对齐** — Profile 参数对标 Chrome DevTools / WebPageTest / tc netem
2. **损伤正交组合** — 延迟、丢包、带宽、黑洞等损伤可自由组合
3. **并发公平对比** — vanilla 与 catcher 通过 Promise.all 在相同网络条件下并发执行
4. **环境变量控制规模** — `THROUGHPUT_REQUESTS`、`WEAK_REQUESTS` 等可在 CI/本地切换规模
5. **零外部依赖** — 所有网络模拟通过 Node.js `net` 模块纯 JS 实现，不需要 tc/netem/代理软件

## 阅读路径

- **新人**：本页 → [`00-overview.md`](./00-overview.md) → 按需深入
- **写测试**：[`03-scenarios.md`](./03-scenarios.md) 看场景模板
- **改损伤**：[`01-proxy.md`](./01-proxy.md) 看损伤模型
- **加 Profile**：[`02-profiles.md`](./02-profiles.md) 看标准值
