# catcher-napi-http/ws DNS 缓存未生效

## 严重程度：P0

DNS 缓存是 catcher 从 TS 版迁移到 Rust NAPI 版时的核心能力之一，架构设计明确定义了缓存参数，但实现时被完全遗漏。此问题导致基于 DNS benchmark 做出的性能判断不可靠。

## 位置

- `catcher-http` (Rust): `packages/catcher-http/src/transport/dns.rs`
- `catcher-http` (Rust): `packages/catcher-http/src/transport/http_client.rs` — 行 69-81
- `catcher-http` (Rust): `packages/catcher-http/src/types/http.rs` — `DnsConfig` 定义
- `klip-electron`: `src/shared/network/catcher-http.ts` — 行 56 `dns: { cache_ttl_secs: 300 }`
- `catcher-napi-ws`: `WsClientConfig` 类型定义中无 DNS 配置项

## 现象

1. `catcher-napi-http` 的 `DnsConfig.cache_ttl_secs` 配置写了但未生效，DNS 缓存参数未接入 resolver
2. `catcher-napi-ws` 的 `WsClientConfig` 没有暴露 DNS 配置选项，WebSocket 连接无法配置 DNS 缓存
3. DNS 查询失败时没有 fallback 到旧缓存的能力，弱网或 DNS 抖动时直接请求失败

## 架构设计 vs 实际实现

### 架构设计文档定义的 DnsConfig（`docs/arch-rs/03-types.md:117-127`）

```rust
pub struct DnsConfig {
    pub cache_size: usize,          // 512
    pub positive_ttl_secs: u64,     // 300
    pub negative_ttl_secs: u64,     // 60
}
```

### 架构设计文档定义的 build_dns_resolver（`docs/arch-rs/04-transport.md:244-255`）

```rust
#[cfg(feature = "hickory-dns")]
pub fn build_dns_resolver(
    config: &DnsConfig,
) -> Result<Option<TokioAsyncResolver>, CatcherError> {
    let resolver = TokioAsyncResolver::builder_tokio()
        .cache_size(config.cache_size)
        .positive_ttl(Some(Duration::from_secs(config.positive_ttl_secs)))
        .negative_ttl(Some(Duration::from_secs(config.negative_ttl_secs)))
        .build()
        .map_err(|e| CatcherError::InvalidConfig(format!("DNS: {e}")))?;
    Ok(Some(resolver))
}
```

设计文档明确规定了三个缓存参数（`cache_size`、`positive_ttl_secs`、`negative_ttl_secs`），并且在 `build_dns_resolver` 中直接传给 hickory-resolver 来构建带缓存的 DNS 解析器。

### 实际代码的 DnsConfig（`packages/catcher-http/src/types/http.rs:176-201`）

```rust
pub struct DnsConfig {
    pub cache_ttl_secs: u32,                         // 设计文档的 3 个字段被合并成 1 个
    pub nameservers: Vec<String>,                     // api-gap-analysis 后新增
    pub host_mapping: HashMap<String, String>,        // api-gap-analysis 后新增
}
```

### 实际代码的 build_dns_resolver（`packages/catcher-http/src/transport/dns.rs:10-18`）

```rust
pub fn build_dns_resolver(config: &DnsConfig) -> Result<Option<()>, CatcherError> {
    if config.host_mapping.is_empty() && config.nameservers.is_empty() {
        return Ok(None);
    }
    Ok(Some(()))  // ← 只做了验证，没有构建任何 resolver
}
```

### 偏差总结

| 维度 | 架构设计 | 实际实现 |
|---|---|---|
| 缓存参数 | `cache_size` + `positive_ttl` + `negative_ttl`（3 个） | `cache_ttl_secs`（1 个，且未使用） |
| Resolver 构建 | 用 hickory `TokioAsyncResolver` 配置缓存参数 | 返回 `Ok(Some(()))` 空壳 |
| 自定义 nameservers | 设计时无，后在 api-gap 补充 | 有独立的 `HickoryDnsResolver`，但不配缓存 |
| host_mapping | 设计时无，后在 api-gap 补充 | 实现了 URL 改写逻辑 |
| DNS 缓存 | 有，由 hickory-resolver 管理，参数可控 | 缓存参数未接入，不可控 |
| 返回类型 | `Option<TokioAsyncResolver>` — 返回真实 resolver | `Option<()>` — 降级为空壳验证函数 |

### 为什么会这样

看代码演变轨迹：

1. **api-gap-analysis 扩展了 DnsConfig 的职责** — 原设计只关注缓存（`cache_size`/`positive_ttl`/`negative_ttl`），gap 分析后增加了 `nameservers` + `host_mapping`，重新定义了 struct
2. **实现时 host_mapping 走了 URL 改写路径** — 在 `http_client.rs:581-596`，host_mapping 被实现为请求级别的 URL 替换 + Host header 覆盖，绕过了 DNS resolver 层
3. **custom_resolver 只转发了 nameservers，没有配置缓存** — `HickoryDnsResolver::new()` 在 `dns.rs:54-72` 用的是 `ResolverOpts::default()`，完全没读 `cache_ttl_secs`
4. **原来的 `build_dns_resolver` 被降级为配置验证函数** — 返回类型从 `Option<TokioAsyncResolver>` 变成了 `Option<()>`，只检查配置是否为空

**简单说：重构 DnsConfig 结构时，把缓存功能搞丢了。新增 nameservers 和 host_mapping 后，focus 全在自定义解析上，原有的缓存设计被遗漏。**

### 研究文档也指出了缓存需求

`docs/research/expandation/02-network-env.md` 明确列出了依赖 DNS 缓存的场景：

- **B3.2 DNS 负载均衡**："DNS cache TTL 到期后获取新 IP list"
- **B4.1 Kubernetes**："Pod 重启后 IP 改变 → DNS cache 需及时刷新"
- **B4.1 容器 DNS**："容器 DNS 超时 → 验证 DNS 超时不阻塞请求"

这些场景都需要一个有缓存且可控 TTL 的 DNS 解析器，但实际上缓存层根本不存在。

## 原因详解

### 问题 1：cache_ttl_secs 字段声明了但没有接入 hickory-resolver

`DnsConfig` 结构体定义了 `cache_ttl_secs: u32`（默认 300），但在 `http_client.rs` 构建 reqwest client 时：

```rust
// http_client.rs:69-77
#[cfg(feature = "hickory-dns")]
if let Some(ref dns) = config.dns {
    build_dns_resolver(dns)?;
    if !dns.nameservers.is_empty() {
        let resolver = crate::transport::dns::build_custom_resolver(&dns.nameservers)
            .map_err(CatcherError::InvalidConfig)?;
        reqwest_builder = reqwest_builder.dns_resolver(resolver);
    }
}
```

- 只用了 `nameservers`（自定义 DNS 服务器）和 `host_mapping`（静态 IP 映射）
- `cache_ttl_secs` 没有传入 `ResolverOpts`，缓存参数不可控

### 问题 2：TS 版有缓存但 NAPI 版丢失了

| 包 | DNS 缓存方案 | 状态 |
|---|---|---|
| `catcher-http-ts` | `cacheable-lookup` npm 包 | 有效，benchmark 验证过 |
| `catcher-napi-http` | `cache_ttl_secs` 字段未接入 | 未按设计实现 |
| `catcher-napi-ws` | 无 DNS 配置项 | 未实现 |

klip-electron 从 `catcher-http-ts` 迁移到 `catcher-napi-http` 后，设计预期的 DNS 缓存能力未实现。之前的 DNS benchmark 测的是 TS 版的 `CacheableLookup`，不适用于当前 Rust NAPI 版。

## 影响

- 架构设计的 DNS 缓存能力未实现，缓存参数不可控
- 弱网环境下 DNS 抖动会导致请求直接失败，没有 fallback 保护
- 基于 DNS benchmark 结果做出的性能判断不可靠 — benchmark 测的是 TS 版，NAPI 版从未有过设计预期的缓存实现

## 测试覆盖缺口

catcher 仓库的 11 个测试/benchmark 文件中，**9 个用的是 TS 版（`@eric8810/catcher-http` / `@eric8810/catcher-ws`），只有 2 个用了 NAPI 版**：

| 文件 | 导入包 | 版本 |
|---|---|---|
| `benchmark/agent.bench.ts` | `@eric8810/catcher-http` | TS |
| `benchmark/codec.bench.ts` | `@eric8810/catcher-ws` | TS |
| `benchmark/throughput.test.ts` | `@eric8810/catcher-http` | TS |
| `integration/dns.test.ts` | `@eric8810/catcher-http` | TS |
| `integration/http.test.ts` | `@eric8810/catcher-http` | TS |
| `integration/ws.test.ts` | `@eric8810/catcher-ws` | TS |
| `e2e/scenarios.test.ts` | `@eric8810/catcher-http` + `@eric8810/catcher-ws` | TS |
| `chaos/chaos.test.ts` | `@eric8810/catcher-http` + `@eric8810/catcher-ws` | TS |
| `chaos/extreme-scenarios.test.ts` | `@eric8810/catcher-http` | TS |
| **`integration/napi.test.ts`** | `@eric8810/catcher-napi-http` + `@eric8810/catcher-napi-ws` | **NAPI** |
| **`e2e/rust-vs-vanilla.test.ts`** | `rust-adapter.ts` → napi | **NAPI** |

关键问题：

1. **DNS benchmark 测的是 TS 版**：`integration/dns.test.ts` 用的 `@eric8810/catcher-http`（TS 版 + `cacheable-lookup`），证明的是 TS 版 DNS 缓存有效，与 NAPI 版无关
2. **S8 DNS 测试有误导性**：`rust-vs-vanilla.test.ts` 的 S8 虽然用了 NAPI 版，但它传了 `dnsNameservers`（走自定义 DNS 路径），与 klip-electron 实际使用路径不同，**S8 结果不代表实际场景**
3. **缺少 NAPI 版的独立 DNS 缓存测试**：没有针对 NAPI 版在无自定义 nameservers 场景下的 DNS 缓存验证

### 需要补充的测试

- NAPI 版 DNS 缓存集成测试（无自定义 nameservers，仅靠 `cache_ttl_secs`）
- NAPI 版 DNS stale-on-error 测试（缓存过期 + DNS 不可达时的 fallback 行为）
- NAPI 版 DNS 缓存 benchmark（与 TS 版 `cacheable-lookup` 对比）
- 所有 TS 版测试应考虑是否需要 NAPI 版镜像测试

## 修复方案：自建 StaleAwareDnsResolver

### 已有依赖

catcher-http 的 `Cargo.toml` 已将 `hickory-dns` 作为默认 feature，依赖 `hickory-resolver` 0.25 和 `hickory-proto` 0.25。hickory-resolver 本身不支持 RFC 8767 serve-stale，无 stale-while-revalidate 能力，因此需要在其上构建缓存包装层。

### 架构

`StaleAwareDnsResolver` 是一个缓存包装层，不涉及任何 DNS 协议实现。实际 DNS 解析（UDP/TCP/DoT/DoH、递归查询、DNSSEC 等）全部委托给 hickory-resolver。

```
StaleAwareDnsResolver (impl reqwest::dns::Resolve)
  ├── inner: HickoryDnsResolver（hickory-resolver，处理所有 DNS 协议细节）
  ├── cache: moka::future::Cache<String, CacheEntry>
  │     └── CacheEntry { addrs: Vec<SocketAddr>, inserted: Instant, ttl: Duration }
  └── config: DnsConfig { cache_size, cache_ttl, negative_ttl, stale_ttl, stale_on_error }
```

### 解析流程（RFC 8767 inspired）

```
resolve(hostname)
  │
  ├─ 查 cache
  │    ├─ 命中且 fresh（在 TTL 内）→ 直接返回
  │    ├─ 命中但 stale（超 TTL 但在 stale_ttl 内）
  │    │    ├─ 返回旧结果（不阻塞调用方）
  │    │    └─ tokio::spawn 后台刷新 → 成功则更新 cache，失败则保持 stale
  │    └─ 未命中 / 超过 stale_ttl
  │         └─ 同步调用 inner.resolve()
  │              ├─ 成功 → 写入 cache → 返回
  │              └─ 失败
  │                   ├─ 有 stale 条目 + stale_on_error=true → 返回旧结果兜底
  │                   └─ 无旧缓存 → 报错
  │
  └─ host_mapping 优先级最高：命中则跳过以上所有步骤，直接返回映射 IP
```

### DnsConfig 扩展

对齐架构设计文档并增加 stale 能力：

```rust
pub struct DnsConfig {
    pub cache_size: usize,          // 缓存条目数（默认 512）
    pub cache_ttl_secs: u32,        // 正常缓存 TTL（默认 300）
    pub negative_ttl_secs: u32,     // 否定缓存 TTL（默认 60）
    pub stale_ttl_secs: u32,        // 过期后仍可用的宽限期（默认 3600）
    pub stale_on_error: bool,       // DNS 失败时是否用旧缓存兜底（默认 true）
    pub nameservers: Vec<String>,
    pub host_mapping: HashMap<String, String>,
}
```

与现有字段的兼容：`cache_ttl_secs` 保留（已被 klip-electron 使用），新增 `cache_size`、`negative_ttl_secs`、`stale_ttl_secs`、`stale_on_error` 均有默认值，不需要调用方改配置。

### 关键实现点

1. **始终构建 hickory-resolver**：即使没有自定义 `nameservers`，也需要通过 `reqwest_builder.dns_resolver()` 注入 `StaleAwareDnsResolver`，否则 reqwest 仍走 `GaiResolver`（无缓存）
2. **`reqwest::dns::Resolve` trait**：`resolve(&self, name: Name) -> Resolving`，`&self` 不可变，缓存需用 `moka::future::Cache`（内部并发安全）
3. **后台刷新不阻塞**：stale 命中时用 `tokio::spawn` 异步刷新，调用方拿到旧结果立即返回
4. **moka cache**：高性能并发缓存，支持 TTL、异步接口、自动淘汰，活跃维护。用它管 TTL 过期和 LRU 淘汰，不需要手写

### 新增依赖

- `moka`（concurrent cache with TTL）— crates.io 周下载量 ~500K，生产级稳定

### 工作量估算

| 模块 | 工作内容 | 代码量 |
|---|---|---|
| `StaleAwareDnsResolver` | resolve 方法 + cache 逻辑 + 后台刷新 | ~100-150 行 |
| `DnsConfig` 扩展 | 新增字段 + serde alias + Default | ~20 行 |
| `http_client.rs` 改造 | 始终构建 resolver 并注入 reqwest | ~15 行 |
| NAPI 层透传 | TS 类型定义 + JSON config 映射 | ~20 行 |
| WsClient DNS 支持 | `WsClientConfig` 增加 `dns` 字段 + 构建 resolver | ~30 行 |
| 测试 | 缓存命中/过期/stale-on-error/benchmark | ~200 行 |

总计约 **400-450 行**，核心逻辑 ~150 行，其余为配置透传和测试。

### WsClient 适配

`catcher-napi-ws` 当前 `WsClientConfig` 没有 DNS 配置项。建议在 `WsClientConfig` 中增加 `dns: DnsConfig` 字段，WsClient 内部构建自己的 `StaleAwareDnsResolver` 实例。HTTP 和 WS 的 resolver 实例各自独立，但共享相同的实现代码。

## 实施清单

一次性全部完成：

- [ ] 实现 `StaleAwareDnsResolver`（缓存包装层 + stale-while-revalidate + stale-on-error）
- [ ] 扩展 `DnsConfig`（新增 `cache_size`、`negative_ttl_secs`、`stale_ttl_secs`、`stale_on_error`）
- [ ] 改造 `http_client.rs`，始终构建 resolver 并注入 reqwest
- [ ] `catcher-napi-ws` 的 `WsClientConfig` 增加 `dns` 字段，复用 `StaleAwareDnsResolver`
- [ ] NAPI 层 TS 类型定义更新
- [ ] 补充 NAPI 版 DNS 测试（缓存命中/过期/stale-on-error/benchmark）
- [ ] 将现有 TS 版测试镜像为 NAPI 版
