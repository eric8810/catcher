# `proxy.mode = "system"` — 自动系统代理检测

> 日期：2026-06-13 · 目标版本：0.3.15

## 背景

catcher v0.3.13 已支持 `ProxyConfig`，但需要调用方手动传入代理 URL。两个核心问题：

1. **调用方不知道代理地址** — Clash Verge 等工具在 OS 层面设置，应用层拿不到
2. **代理地址会变** — 切换 Clash 配置/节点，地址从 `127.0.0.1:7890` 变 `127.0.0.1:7891`

**目标**：catcher 提供 `proxy.mode = "system"`，自动从 OS 读取。`networkChanged()` 时重检。

---

## 现状

### catcher 当前 ProxyConfig（`catcher-core/src/types/network.rs`）

```rust
pub struct ProxyConfig {
    pub url: String,            // 必填
    pub auth: Option<ProxyAuth>,
    pub no_proxy: Vec<String>,
}
```

- `networkChanged()` 不重检 proxy — config 以 `Arc<HttpClientConfig>` 传入，构建后不可变
- `build_middleware_client()` 调 `transport_url()` 取代理 URL 传给 `reqwest::Proxy::all(url)`

### reqwest 自带 system-proxy 用不上

reqwest 的 system-proxy feature 委托给 `hyper-util`，macOS/Windows 确实读 OS 代理。但 catcher 用 `reqwest::Proxy::all(url)` 手动构建 → 完全绕过 reqwest 的系统检测（reqwest 只在未显式设 proxy 时才自动检测）。catcher 需要一个"给 reqwest 指令"的中间层 — 即自行检测 OS 代理再传给 `reqwest::Proxy::all()`。

### 生态：`proxy-cfg` v0.4.2

MIT/Apache-2.0，Devolutions 维护。全平台 OS 代理读取：

| 平台 | API |
|------|-----|
| macOS | `SCDynamicStoreCopyProxies()` (HTTP/HTTPS/FTP) |
| Windows | WinINET 注册表 + WinHTTP |
| Linux | 环境变量 + `/etc/sysconfig/proxy` |

**已知缺口**：macOS 不读 SOCKS 代理。Clash Verge 通常同时设 HTTP+SOCKS，HTTP 路径能覆盖。仅开 SOCKS 的场景后续补。

---

## 方案设计

### API

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    Manual,   // 默认，向后兼容
    System,   // OS 自动检测
}

impl Default for ProxyMode {
    fn default() -> Self { Self::Manual }
}

pub struct ProxyConfig {
    #[serde(default)]
    pub mode: ProxyMode,
    pub url: Option<String>,    // Manual 必填，System 忽略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
    #[serde(alias = "noProxy", default)]
    pub no_proxy: Vec<String>,
}
```

- `url: String` → `Option<String>` 是唯一 breaking change（pre-1.0 可接受，改量小）
- `#[serde(default)]` 确保旧 JSON 不传 `mode` → 默认 `Manual`，完全向后兼容
- `transport_url()` 已有 `socks5://` → `socks5h://` 自动转换

### TS/napi 侧

```typescript
// 现有（向后兼容）
new HttpClient({ proxy: { url: 'socks5://127.0.0.1:7890' } })

// 新增
new HttpClient({ proxy: { mode: 'system' } })
```

### 实现架构

```
ProxyConfig { mode: System, url: None }
    │
    ▼
detect_system_proxy()
    ├── macOS: SCDynamicStoreCopyProxies() → HTTP/HTTPS proxy URL
    ├── Windows: WinINET registry + WinHTTP → proxy URL
    └── Linux: env vars + /etc/sysconfig/proxy → proxy URL
    │
    ▼
ProxyConfig { mode: Manual, url: Some("socks5://127.0.0.1:7890"), no_proxy: [...] }
    │
    ▼
reqwest::Proxy::all(transport_url())  // socks5 → socks5h
    .no_proxy(...)
```

`networkChanged()`:
```
if config.proxy.mode == System:
    detect_system_proxy() → new_proxy  // 重新检测
    if new_proxy != cached → rebuild reqwest client
clear DNS cache → reset circuit breaker
```

### `networkChanged()` 重新检测 — HTTP 侧

```rust
pub fn network_changed(&self) -> Result<(), CatcherError> {
    // DNS
    #[cfg(feature = "hickory-dns")]
    if let Some(ref resolver) = self.dns_resolver {
        let _ = resolver.network_changed();
    }

    // 熔断器
    self.network_generation.fetch_add(1, Ordering::SeqCst);
    if let Some(ref cb) = self.circuit_breaker { cb.reset(); }

    // System 模式：重检代理
    let config = if self.config.proxy.as_ref().map_or(false, |p| p.mode == ProxyMode::System) {
        let mut new_config = (*self.config).clone();
        new_config.proxy = detect_and_convert_system_proxy();
        Arc::new(new_config)
    } else {
        self.config.clone()
    };

    let new_client = build_middleware_client(&config, &self.metrics, ...)?;
    *self.client.write() = new_client;
    Ok(())
}
```

- 不更新 `self.config`（仍是原始 config + `mode: System`），下次 `networkChanged()` 重新检测
- System 模式下 `url: None` 在 `build_middleware_client` 走不到 proxy 逻辑（`new_config.proxy` 已被 `detect_and_convert_system_proxy()` 填充为 Manual 或置为 None）

### `networkChanged()` 重新检测 — WS 侧

WS 通过 `cmd_tx.send(WsCommand::NetworkChanged)` 入队，事件循环已自带防抖（排空积压）。重建 reqwest client 时同理重检。

### 代理变化检测策略

**不做主动推送**。原因：三平台通知机制不统一且生命周期复杂。

**改在 `networkChanged()` 时重检**。覆盖度：

| 场景 | 触发 |
|------|------|
| Clash 启动/关闭 | 创建/销毁虚拟网卡 → OS 网络事件 |
| Clash 切换节点 | 代理地址通常不变（`127.0.0.1:7890`），无需处理 |
| VPN 连接/断开 | OS 网络事件 |
| WiFi↔蜂窝 | OS 网络事件 |
| 仅改代理设置不改网络 | ❌ 漏掉，但罕见。重启或手动 `networkChanged()` 即可 |

### 依赖

`proxy-cfg` 通过 `catcher-dns` 中转，http 和 ws 不直接依赖：

```toml
# catcher-dns/Cargo.toml
[features]
system-proxy = ["dep:proxy_cfg"]

# catcher-http/Cargo.toml & catcher-ws/Cargo.toml
[features]
system-proxy = ["catcher-dns/system-proxy"]
```

`detect_system_proxy()` 定义在 `catcher-dns/src/proxy.rs`，单份实现，http/ws 共享。

### `detect_system_proxy()` 映射逻辑

```rust
fn detect_system_proxy() -> Option<ProxyConfig> {
    let os = proxy_cfg::get_proxy_config().ok()??;

    // 优先级 https > http > * (all)
    let url = os.proxies.get("https")
        .or_else(|| os.proxies.get("http"))
        .or_else(|| os.proxies.get("*"))?
        .clone();

    // 检测 scheme 前缀：如果 URL 不含 ://，补 socks5://
    let url = if url.contains("://") { url } else { format!("socks5://{url}") };

    Some(ProxyConfig {
        mode: ProxyMode::Manual,
        url: Some(url),
        auth: None,
        no_proxy: os.whitelist.into_iter().collect(),
    })
}
```

- `transport_url()` 后续自动 `socks5://` → `socks5h://`
- `no_proxy` 从 OS whitelist 合并，与用户显式传入的 `no_proxy` 取并集

### 已知限制

| 限制 | 影响 | 后续 |
|------|------|-----|
| macOS 不读 SOCKS | 仅开 SOCKS 代理时检测不到 | `proxy-cfg` PR 或自行补 `SCDynamicStoreCopyProxies` SOCKS key |
| Linux 不读 gsettings | GNOME GUI 设的代理检测不到 | `zbus` 或 `gsettings` 命令行 |
| PAC/WPAD 不支持 | 企业 PAC 脚本无法解析 | 需要 JS 引擎，不是 catcher 定位 |
| 代理认证不支持 | `proxy-cfg` 不读 OS 认证信息 | OS 级代理通常无需认证 |

---

## 改动清单

| Crate | 文件 | 改动 |
|-------|------|------|
| `catcher-core` | `types/network.rs` | `ProxyMode` enum + `ProxyConfig.url` → `Option<String>` + `mode` 字段 |
| `catcher-http` | `Cargo.toml` | `system-proxy` feature + `proxy-cfg` dep |
| `catcher-http` | `transport/proxy.rs` | **新增** `detect_system_proxy()` |
| `catcher-http` | `transport/http_client.rs` | `network_changed()` System 模式重检 |
| `catcher-ws` | `Cargo.toml` | 同上 |
| `catcher-ws` | `transport/ws_client.rs` | `network_changed()` System 模式重检 |
| `catcher-napi-http` | binding | `ProxyConfig` JSON serde 适配 |
| `catcher-napi-ws` | binding | 同上 |
| `catcher-ffi` | C ABI + UniFFI UDL | `ProxyConfig` 结构体适配 |

---

## 测试

- [ ] macOS 设 HTTP 代理 → `detect_system_proxy()` 返回正确 URL
- [ ] macOS 取消代理 → 返回 `None`
- [ ] Windows `ProxyEnable=1` → 检测到 `ProxyServer`
- [ ] Windows `ProxyEnable=0` → 返回 `None`
- [ ] Linux `HTTP_PROXY` env → 检测到
- [ ] 无代理 + `mode=System` → 退化为直连，不影响
- [ ] `networkChanged()` 后代理从无到有 → 新请求走代理
- [ ] `networkChanged()` 后代理从有到无 → 新请求走直连
- [ ] Clash socks5 → `socks5://` 自动转 `socks5h://`
- [ ] 旧 JSON（无 `mode` 字段）→ 反序列化正常，行为不变
