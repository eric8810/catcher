# 移动端 Clash / VPN / 本地代理下请求失败

**严重程度**: 🔴 P0

**状态**: Catcher 侧已实现，待移动端和集成验证

**更新时间**: 2026-06-12

**影响范围**: `catcher-http`、`catcher-ws`、`catcher_core` Dart FFI、`catcher-napi-http`、`catcher-napi-ws`，以及接入方 `echoo-flutter` / `klip-electron`

## 现象

客户在 iOS 和 Android 上开启 Clash、VPN 或本地代理后，集成 Catcher 的客户端出现 HTTP 请求失败，同时 WebSocket 也无法连接。

修复前，这个问题不是单一 bug，而是几类问题叠加：

1. Catcher 不会自动读取移动系统当前的代理设置。
2. HTTP 只有调用方显式传 `proxy` 时才走代理。
3. HTTP 类型声明写了 `socks5://`，但 crate 没启用 `reqwest/socks`。
4. 修复前 WebSocket 没有代理通道，连接流程仍然是解析域名后直连目标 IP。
5. 修复前 HTTP 和 WebSocket 默认都会使用 Catcher 自己的 DNS 解析与缓存，不保证跟随 iOS / Android 当前 VPN 网络的 DNS 行为。
6. 接入项目没有把系统代理、VPN 状态、代理证书、网络切换事件完整传给 Catcher。

## 一句话结论

开启 Clash / VPN / 本地代理后，系统希望 HTTP 和 WebSocket 都按当前网络策略走；修复前 Catcher 仍按自身配置创建连接，HTTP 没走进代理，WebSocket 也没有代理连接流程，DNS 还可能和系统当前网络不一致，所以两类请求都会失败。

本次 Catcher 侧修复后：

- 不配置 `dns` 时，HTTP / WS 都走 reqwest 默认解析路径，不再默认强行接入 Catcher DNS。
- 显式配置 `dns.mode = "catcher"` 时，才启用 Catcher DNS 缓存、host mapping 和自定义 nameserver。
- 读取系统 DNS 失败时，不再静默退回 Hickory 默认 DNS；只有显式打开 `fallback_to_default_nameservers` 才允许退回。
- HTTP 启用 `reqwest/socks`，`socks5://` 和 `socks5h://` 与类型声明一致。
- WebSocket 改用 yawc 的 reqwest 建连入口，和 HTTP 一样接入 proxy / TLS / DNS 配置。
- Dart FFI、NAPI TS、公共 TS 类型已暴露 `dns.mode`、`proxy`、`tls`、`network_path_id` 等字段。

## 补充判断：为什么 escape 也可能没用

如果不使用 Catcher 时客户端正常，且 Clash 的 escape 规则可以让 App 跳过代理直连；一旦使用 Catcher 后 escape 也没用，最可疑的点不是“直连服务器一定不通”，而是 Catcher 在真正连接服务器之前已经改变了网络路径。

按现有代码和现象看，问题优先级应这样排：

1. **第一怀疑：DNS 在 Catcher 内部先失败或走错。**
   HTTP 和 WebSocket 都会先接入 Catcher 自己的 DNS resolver。移动端如果读取系统 DNS 失败，会退回 Hickory 默认 DNS；Hickory 默认是 Google DNS。这样可能导致请求卡在 DNS 阶段，业务 HTTP / WSS 连接根本还没发出去。
2. **第二怀疑：Catcher 提前把域名解析成 IP，Clash 规则失去上下文。**
   Clash 的 escape / 分流规则常常依赖 App、域名、fake-ip 或目标地址。旧链路把域名和连接交给系统网络栈，Clash 能判断；Catcher 先解析域名、再按 IP 连接，规则看到的内容可能已经变了。
3. **第三怀疑：HTTP / WebSocket 没有完整代理连接流程。**
   HTTP 只有显式传 `proxy` 才走代理，当前 echoo 没传；WebSocket 没有 HTTP CONNECT、SOCKS5、远端 DNS 流程，所以遇到必须走代理的模式会失败。
4. **第四怀疑：VPN / Clash 开关后，旧 DNS resolver、旧连接池和旧 WS 连接没有重建。**
   Clash 模式、VPN、Wi-Fi、蜂窝网络变化后，DNS、路由和代理都可能变。Catcher 继续使用旧对象，也会表现为“全都发不出去”。

需要特别说明：在 Android 上，如果 Clash 的 escape 是基于 `VpnService` 的 App 排除，Rust FFI 发出的 socket 通常仍属于同一个 App UID，所以“Rust native socket 不算 App 流量”不是第一怀疑点。更可疑的是 Catcher 在 socket 建立前已经改变了 DNS 和连接方式。iOS 上也应先按这个方向验证，再看具体 Clash 类 App 对规则的实现差异。

最可能的链路是：

1. 旧链路使用 Dart / 系统网络栈，DNS 和连接都按 iOS / Android 当前网络规则走，Clash 能识别 App、域名或连接目标，所以 escape 可以生效。
2. Catcher HTTP 创建 reqwest client 时，总是接入自己的 DNS resolver。
3. 这个 resolver 在移动端如果读取系统 DNS 配置失败，会退回 Hickory 默认 DNS；Hickory 默认是 Google DNS。
4. 于是请求可能先卡在 DNS 查询，根本没有走到“连接业务服务器”这一步。
5. 即使 DNS 成功，如果拿到的是真实 IP，而 Clash 的 escape 规则依赖域名、fake-ip 或进程识别，也可能导致规则没有命中。

因此，现场的第一怀疑点应是 **Catcher DNS resolver 与 Clash / 系统 DNS 路径不一致**。第二怀疑点才是 **HTTP / WebSocket 连接没有按代理配置建立**。

## Catcher 必须做的事情

这些事情不依赖进一步复现。只要 Catcher 要支持 Clash、VPN、本地代理，就都应该在库内补齐。

1. **把 DNS 行为改成显式策略，不能继续静默退回 Google DNS。** ✅ 已实现
   移动端读取系统 DNS 失败时，不能悄悄使用 Hickory 默认 DNS。必须让调用方知道当前使用的是系统 DNS、平台传入 DNS、代理 DNS、自定义 DNS，还是 Catcher DNS 缓存。
2. **HTTP 和 WebSocket 使用同一份网络配置。** ✅ 已实现
   不能 HTTP 一套代理/DNS/TLS，WebSocket 又一套。统一配置必须同时表达代理类型、DNS 模式、TLS 配置和网络路径版本。
3. **WebSocket 补齐代理能力。** ✅ 已实现
   WS / WSS 必须支持 HTTP proxy `CONNECT`、HTTPS proxy `CONNECT`、SOCKS5、SOCKS5 远端 DNS。否则只要客户的网络要求 WebSocket 走代理，必然会失败。
4. **HTTP 补齐真实 SOCKS 能力。** ✅ 已实现
   当前类型上写了 `socks5://`，但底层没有启用对应能力，必须让 API 和实际能力一致。
5. **代理场景下不能提前乱解析目标域名。** ✅ 已实现
   该让代理解析时就让代理解析。尤其是 `socks5h://`、fake-ip、远端 DNS、按域名分流这些场景，Catcher 不能先把域名改成真实 IP。
6. **VPN / 代理 / DNS 变化后重建关键对象。** ✅ 配置入口已实现，外部接入负责触发
   Catcher 已暴露 `network_path_id`。网络路径一变，外部项目应传入新值并重建 HTTP client、WS client 和相关连接。
7. **补齐可观测日志和错误分类。** 🟡 仍是验证项
   当前已有 DNS、TLS、WS 握手等错误分类；更细的现场日志还需要结合接入项目和真机验证补齐。

## 外部集成项目要做的事情

外部项目不应该自己临时实现一套代理连接逻辑。它们要做的是把平台当前网络状态准确传给 Catcher，并在网络变化后重建 HTTP / WebSocket 客户端。

详细工作已拆到对应项目：

- `echoo-flutter`：`../echoo-flutter/docs/issues/catcher-mobile-proxy-vpn-clash-integration.md`
- `klip-electron`：`../klip-electron/docs/issues/catcher-proxy-vpn-clash-integration.md`

## Catcher 侧验证项

这些是为了确认根因，不是最终修复方案。

1. **验证是不是 DNS 阶段就失败。**
   在 Catcher DNS 前后打日志：host、DNS 模式、系统 DNS 是否读取成功、nameserver、是否用了默认 DNS、解析耗时、解析结果、错误类型。如果服务器和 Clash 都没有看到连接，但 DNS 日志失败，根因就在 DNS 前半段。
2. **验证禁用 Catcher DNS 后是否恢复。**
   只做一次诊断实验：让 HTTP / WS 不注入 Catcher resolver，改用平台默认解析。如果 Clash escape 下立刻恢复，基本可以确认是 Catcher DNS 路径破坏了规则。
3. **验证解析结果是否和旧链路不同。**
   对同一个域名，对比旧链路、系统解析、Catcher 解析、Clash fake-ip 下的结果。如果 Catcher 拿到真实 IP，而旧链路走 fake-ip 或代理 DNS，说明规则上下文被改变。
4. **验证请求有没有进入 TCP 连接阶段。**
   日志记录 DNS 成功后是否开始连接、连接目标是域名还是 IP、目标端口、是否使用代理、代理类型。如果没有 TCP 连接日志，就不是服务器拒绝，而是连接前失败。
5. **验证 TLS 是否是后续问题。**
   如果 DNS 成功、TCP 也连上，但 HTTP / WSS 握手失败，再看证书链、自定义 CA、`rejectUnauthorized` 和 WSS TLS 配置。TLS 不是当前第一怀疑点，但要纳入验证。
6. **验证网络切换后是否仍用旧对象。**
   打印网络路径版本，打开 / 关闭 Clash 或 VPN 后，检查 HTTP client、DNS resolver、WS client 是否重建。如果版本变了对象没变，就是明确问题。

## 修复前源码证据

### HTTP 只支持显式代理

位置：`packages/catcher-http/src/transport/http_client.rs`

HTTP 仍然只在 `config.proxy` 不为空时才设置 reqwest 代理：

```rust
if let Some(ref proxy_config) = config.proxy {
    let mut proxy = reqwest::Proxy::all(&proxy_config.url)?;
    reqwest_builder = reqwest_builder.proxy(proxy);
}
```

这说明：

- 调用方不传 `proxy`，Catcher HTTP 不会主动知道 Clash、本地代理或 App 内代理设置。
- 一旦调用 `proxy()`，reqwest 的自动系统代理逻辑也不会再接管。

修复后，`packages/catcher-http/Cargo.toml` 已启用 `reqwest/socks`，因此显式传入 `socks5://` 或 `socks5h://` 时底层能力和类型声明一致。

修复前的位置：`packages/catcher-http/Cargo.toml`

```toml
reqwest = { version = "0.13", default-features = false, features = [
    "http2", "gzip", "brotli", "deflate", "stream", "charset", "query", "form"
] }
```

修复前没有启用：

- `reqwest/socks`
- `reqwest/system-proxy`

但类型定义里写了：

```rust
/// "http://host:port" | "https://host:port" | "socks5://host:port"
pub struct ProxyConfig {
    pub url: String,
}
```

所以修复前 `socks5://` 是高风险路径：API 看起来支持，但底层 feature 没打开。

### 修复前 WebSocket 没有代理连接流程

位置：`packages/catcher-ws/src/transport/ws_client.rs`

修复前 WebSocket 会先用 Catcher DNS 得到目标地址，再直连该地址：

```rust
let addrs = resolver.resolve_socket_addrs(&host, port).await?;
...
yawc::WebSocket::connect(url)
    .with_tcp_address(addr)
    .with_options(ws_config)
    .with_request(request)
    .await
```

这条路径没有：

- HTTP proxy `CONNECT`
- HTTPS proxy `CONNECT`
- SOCKS5
- SOCKS5 远端 DNS
- PAC
- 系统代理读取

因此修复前 WSS 在本地代理、企业代理、Clash HTTP/SOCKS 端口下都不能可靠工作。

修复后，WebSocket 使用 `yawc::WebSocket::reqwest(...)` 建连。`reqwest::Client` 统一承接 proxy、TLS、DNS resolver 和默认握手 headers，初连、多端点竞速、重连都走同一条路径。

### 修复前 DNS 默认走 Catcher 自己的解析器

位置：`packages/catcher-http/src/transport/http_client.rs`

```rust
let dns_config = config.dns.clone().unwrap_or_default();
let resolver = crate::transport::dns::build_stale_aware_resolver(&dns_config)?;
reqwest_builder = reqwest_builder.dns_resolver(resolver);
```

位置：`packages/catcher-ws/src/transport/ws_client.rs`

```rust
let dns_config = config.dns.clone().unwrap_or_default();
build_stale_aware_resolver(&dns_config)
```

位置：`packages/catcher-dns/src/lib.rs`

修复前默认启用 `hickory-dns` 时，会读取系统 DNS 配置并建立自己的缓存：

```rust
let (sys_config, sys_opts) = hickory_resolver::system_conf::read_system_conf()
    .unwrap_or_else(|_| (ResolverConfig::default(), ResolverOpts::default()));
```

这个设计对普通弱网有价值，但在移动 VPN 场景下有风险：

- iOS / Android 的 DNS 可能是按当前网络、当前 VPN、当前 App 路由动态决定的。
- Clash 的 fake-ip、分流、远端 DNS 等策略可能不等同于本地 `system_conf`。
- VPN 开关变化时，已有 DNS resolver 和连接池不会自动重建。
- 如果代理应该负责解析域名，本地提前 DNS 解析会破坏代理策略。

修复后：

- `config.dns == None` 时，HTTP / WS 不注入 Catcher resolver，使用 reqwest 默认解析路径。
- `dns.mode = "catcher"` 时，才使用 Catcher DNS 缓存、host mapping、自定义 nameserver。
- `dns.mode = "native"` 保留为显式原生解析意图。
- `fallback_to_default_nameservers` 默认是 `false`，不会静默退回 Hickory 默认 DNS。

### 修复前 WebSocket TLS 证书配置不够

位置：`yawc-0.3.3/src/native/mod.rs`

yawc 默认 WSS TLS 连接使用 `webpki_roots`：

```rust
root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter()...)
```

Catcher HTTP 已经有 `TlsConfig`，接入方也能设置 `rejectUnauthorized` 或自定义 CA；但修复前 WebSocket 配置没有同等 TLS 设置。遇到调试代理、企业代理或本地 MITM 代理时，WSS 更容易因为证书不被信任而失败。

修复后，`WsClientConfig` 已增加 `tls`。当前支持证书校验开关、自定义 CA、客户端 PEM 证书、TLS 版本限制；`pin_sha256` 在 WS 路径仍需要后续单独实现，不在本次移动代理修复里冒充已完成。

## echoo-flutter 实际接入情况

### 版本

位置：`../echoo-flutter/pubspec.yaml`

```yaml
catcher_core: ^0.3.12
```

移动端已经接的是当前较新的 Dart FFI 包，所以这不是“版本太旧”单独造成的问题。

### Catcher 默认开启

位置：`../echoo-flutter/lib/main.dart`

```dart
bool catcherCoreEnabled = sp.getBool(SP_CATCHER_CORE_ENABLED) ?? true;
CatcherCoreHttpOptions.enabled = catcherCoreEnabled;
CatcherCoreSocketOptions.enabled = catcherCoreEnabled;
```

默认启用意味着客户没有主动关闭时，HTTP 和 WebSocket 都可能走 Catcher。

### HTTP 只在 App 内手动代理打开时回退 Dio

位置：`../echoo-flutter/lib/core/network/catcher_core_http_routing.dart`

```dart
if (proxyEnabled) {
  return 'proxyEnabled';
}
```

位置：`../echoo-flutter/lib/api/api_const.dart`

```dart
httpProxyIsOpen = pref.getBool(SP_HTTP_PROXY_ISOPEN) ?? false;
...
client.findProxy = (uri) => 'PROXY $httpProxyIP:$httpProxyPort';
```

这说明：

- App 内开发设置里的 HTTP 代理打开后，HTTP 会回退到 Dio。
- 但 Clash / VPN 的系统代理不一定会写入这些 SharedPreferences。
- Catcher HTTP 的创建代码没有传 `ProxyConfig`。

位置：`../echoo-flutter/lib/core/network/catcher_core_http_transport.dart`

```dart
catcher.HttpClientConfig(
  baseUrl: config.baseUrl,
  ...
  tls: catcher.TlsConfig(rejectUnauthorized: config.rejectUnauthorized),
)
```

这里没有传：

- `proxy`
- `dns`
- 当前是否 VPN
- 当前是否系统代理

所以客户开启 Clash/VPN 后，HTTP 很可能仍然走 Catcher 的默认直连和默认 DNS。代理要求请求先进本地代理，但 HTTP 实际没有进去，就会表现为“开代理后 HTTP 也没网”。

### WebSocket 只按协议决定是否走 Catcher

位置：`../echoo-flutter/lib/core/network/catcher_core_socket_routing.dart`

```dart
return uri.scheme == 'ws' || uri.scheme == 'wss';
```

位置：`../echoo-flutter/lib/core/network/catcher_core_socket_transport.dart`

```dart
catcher.WsClientConfig(
  urls: [config.uri.toString()],
  perMessageDeflate: true,
  handshakeTimeoutMs: config.timeout.inMilliseconds,
  reconnect: catcher.WsReconnectConfig(...),
  headers: ...,
  protocols: config.protocols,
)
```

这里没有代理参数，也没有按 VPN / 代理状态回退旧 WebSocket。

### 网络恢复只覆盖离线到在线

位置：`../echoo-flutter/lib/model/web_socket_model.dart`

```dart
return isOnline && wasOnline == false;
```

这能改善断网恢复，但 VPN 开关、Clash 模式切换、Wi-Fi 到 VPN、VPN 到 Wi-Fi 等情况通常不是简单的离线到在线。因此已有连接、DNS 缓存、WebSocket 连接不一定会重建。

### 现有弱网报告已经提示 WebSocket 不宜全量

位置：`../echoo-flutter/weak-network/catcher-vs-legacy-ab-test-report.md`

报告结论是：

- HTTP 可以继续小流量灰度。
- WebSocket 暂缓全量切换。
- 需要先修 `handle not found` 和 reconnect 状态机。

代理/VPN 问题会进一步放大 WebSocket 风险。

## klip-electron 实际接入情况

### 版本

位置：`../klip-electron/package.json`

```json
"@eric8810/catcher-napi-http": "0.3.10",
"@eric8810/catcher-napi-ws": "0.3.10"
```

桌面端当前还在旧版本。后续需要先升级，再接代理能力。

### HTTP 没有传代理

位置：`../klip-electron/src/shared/network/catcher-http.ts`

```ts
new HttpClient({
  base_url: baseURL,
  tls: { reject_unauthorized: false },
  dns: { cache_ttl_secs: 300 },
  retry: { max_attempts: 3, backoff: 'Exponential' },
})
```

这里显式配置了 DNS 缓存，但没有传代理。

### WebSocket 没有传代理

位置：`../klip-electron/src/main/presenter/imPresenter/websocket.ts`

```ts
new WsClient({
  urls: [envHelper.URL_GATEWAY_WS],
  ...
  reconnect: { ... },
  heartbeat: { ... },
  dns: { cache_ttl_secs: 300 },
})
```

位置：`../klip-electron/src/main/presenter/deepSeek/websocket.ts`

```ts
new WsClient({
  urls: [envHelper.URL_GATEWAY_AI_WS],
  ...
  dns: { cache_ttl_secs: 300 },
})
```

NAPI 接入也没有代理设置。桌面端如果遇到系统代理或企业代理，也会遇到同类问题。

### WebSocket 重连边界不能被破坏

位置：`../klip-electron/docs/issues/im-websocket-napi-reconnect-heartbeat-conflict.md`

已有结论是：

- NAPI 管连接、重连、心跳。
- 应用层管业务 token。
- 4003 token 过期必须由应用层刷新 token 后新建连接。

因此代理/VPN 修复不能简单地在应用层疯狂重连。正确做法是底层连接能力补齐，应用层只在网络路径变化时销毁并重建。

## 根因归类

| 类别 | 根因 | 影响 |
|---|---|---|
| 代理 | HTTP 没拿到代理配置，WS 没代理连接流程 | 本地代理、Clash HTTP/SOCKS 端口下 HTTP 和 WS 都失败 |
| SOCKS | HTTP 未启用 `reqwest/socks` | `socks5://` 看起来支持，实际可能失败 |
| 系统代理 | 没有 iOS / Android 系统代理读取 | Clash 系统代理、企业代理不会自动生效 |
| DNS | 默认使用 Catcher resolver 和缓存 | VPN DNS、fake-ip、split tunnel 下可能错路由 |
| 网络变化 | 没有统一的网络路径变化事件 | VPN 开关后旧连接池和 DNS 缓存继续使用 |
| TLS | WS 缺少 TLS 配置 | WSS 经过调试代理或企业 CA 时失败 |
| 接入层 | echoo / klip 都没有传代理 | 库能力即使部分存在，也没有被使用 |

## 最终解决方案

目标不是做临时回退，而是让 Catcher 在代理 / VPN 场景下自己走对路。Catcher 负责连接能力和统一配置；外部项目负责把平台当前网络状态传进来。

### 目标态

```
iOS / Android / Electron
        │
        ▼
外部项目平台网络探测层
读取 VPN、系统代理、PAC 结果、证书设置、网络路径变化
        │
        ▼
Catcher 统一网络配置
统一表达 direct / http proxy / socks5 / socks5h / vpn / dns / tls
        │
        ├── HTTP Transport
        │   └── 按统一网络配置建连接
        │
        └── WS Transport
            └── 按同一份统一网络配置建连接
```

### Catcher 必须一次补齐的能力

1. 定义统一网络配置结构。✅ 已实现
   - 表达当前连接方式：直连、HTTP 代理、HTTPS 代理、SOCKS5、SOCKS5 远端 DNS。
   - 表达 DNS 策略：默认原生解析、Catcher DNS、自定义 nameserver、host mapping。
   - 表达 TLS 策略：是否校验证书、自定义 CA、客户端 PEM 证书、TLS 版本。
   - 表达网络路径版本：VPN / Wi-Fi / 蜂窝 / 代理切换时递增，用来触发连接池和 DNS 缓存重建。

2. HTTP 传输层完整支持代理。✅ 已实现
   - `catcher-http` 启用 `reqwest/socks`。
   - 支持 `http://`、`https://`、`socks5://`、`socks5h://`。
   - `socks5://` 使用本地 DNS。
   - `socks5h://` 使用代理远端 DNS。
   - 代理 URL 错误、DNS、TLS、WS 握手等会进入明确错误分类；更细现场日志留到真机验证补充。

3. WebSocket 传输层完整支持代理。✅ 已实现
   - 通过 yawc 的 reqwest 建连入口复用 reqwest 代理能力。
   - HTTP/HTTPS 代理、SOCKS5、SOCKS5 远端 DNS 都由同一个 reqwest client 承接。
   - 支持 `socks5h://`，避免代理场景下提前本地解析目标域名。
   - 初连、多端点竞速、重连都使用同一个 reqwest client。

4. WebSocket TLS 配置对齐 HTTP。✅ 主要能力已实现
   - `WsClientConfig` 增加 `tls`。
   - 支持 `reject_unauthorized`。
   - 支持自定义 CA。
   - 支持客户端 PEM 证书和 TLS 版本限制。
   - `pin_sha256` 仍需后续单独实现。
   - WSS 经过调试代理或企业代理时，可以按配置通过验证。

5. DNS 逻辑按连接方式选择。✅ 已实现
   - 默认：不配置 `dns` 时使用 reqwest 原生解析。
   - 显式 Catcher DNS：`dns.mode = "catcher"` 时使用 Catcher DNS 缓存和旧缓存兜底。
   - VPN：默认重新读取系统 DNS，并在网络路径变化后重建 resolver。
   - HTTP proxy：目标域名交给代理隧道，不提前改写成目标 IP。
   - SOCKS5：按 `socks5://` 或 `socks5h://` 决定本地解析还是代理解析。
   - 自定义 nameserver 和 host mapping 仍然保留。

6. 网络路径变化时重建关键对象。✅ Catcher 配置已支持，外部接入负责触发
   - VPN 开关变化。
   - 系统代理变化。
   - Wi-Fi / 蜂窝切换。
   - DNS 配置变化。
   - 以上变化发生后，外部项目传入新的 `network_path_id` 并重建 HTTP / WS client。

### 外部集成项目文档

- `echoo-flutter`：`../echoo-flutter/docs/issues/catcher-mobile-proxy-vpn-clash-integration.md`
- `klip-electron`：`../klip-electron/docs/issues/catcher-proxy-vpn-clash-integration.md`

外部项目只接入目标能力：启动和网络变化时生成统一网络配置，传给 Catcher HTTP / WS。WebSocket 业务边界保持不变：Catcher 管连接、代理、DNS、TLS、重连、心跳；应用层管 token 刷新和业务消息。

### 修复完成后的行为

| 场景 | Catcher 应该怎么走 |
|---|---|
| 普通网络 | 直连，默认使用 reqwest 原生解析 |
| Clash HTTP 代理 | HTTP 和 WSS 都先连本地代理，再由代理访问目标 |
| Clash SOCKS5 | HTTP 和 WSS 都走 SOCKS5 |
| Clash fake-ip / 远端 DNS | 使用 `socks5h://` 或代理 DNS，不提前本地解析目标 |
| VPN 模式 | 按当前 VPN 网络的 DNS 和路由重建连接 |
| 代理证书 | HTTP 和 WSS 都按同一份 TLS 配置校验证书；WS pinning 后续补齐 |
| VPN / 代理切换 | 旧连接池、旧 WS、旧 DNS resolver 全部重建 |

## 测试矩阵

修复后至少覆盖这些场景：

| 平台 | 场景 | HTTP | WSS |
|---|---|---:|---:|
| Android | 无代理、普通网络 | ✅ | ✅ |
| Android | Clash VPN 模式 | ✅ | ✅ |
| Android | Clash HTTP 代理 | ✅ | ✅ |
| Android | Clash SOCKS5 代理 | ✅ | ✅ |
| Android | VPN 开关后重连 | ✅ | ✅ |
| iOS | 无代理、普通网络 | ✅ | ✅ |
| iOS | Clash VPN 模式 | ✅ | ✅ |
| iOS | HTTP 代理 + 自签 CA | ✅ | ✅ |
| iOS | 系统代理开关变化 | ✅ | ✅ |
| Electron | 系统 HTTP/HTTPS 代理 | ✅ | ✅ |
| Electron | 系统 DIRECT | ✅ | ✅ |

额外需要覆盖：

- `socks5://` 本地 DNS
- `socks5h://` 远端 DNS
- HTTP proxy `CONNECT`
- 代理返回 407
- 代理不可达
- DNS fake-ip
- 网络从 Wi-Fi 切到 VPN
- 网络从 VPN 切回 Wi-Fi
- WSS 自定义 CA

## 验收标准

- [x] `catcher-http` 启用 `reqwest/socks`。
- [x] `catcher-ws` 通过 reqwest 支持 HTTP proxy `CONNECT`。
- [x] `catcher-ws` 通过 reqwest 支持 SOCKS5。
- [x] HTTP 和 WS 都支持 `socks5h://` 远端 DNS。
- [x] HTTP 和 WS 都暴露 `network_path_id`，外部可在网络变化后重建连接和 DNS resolver。
- [ ] `catcher-http` 的 `socks5://` 有真实代理集成测试。
- [ ] `catcher-ws` 的 HTTP proxy / SOCKS5 有真实代理集成测试。
- [ ] iOS / Android 真机分别通过 Clash VPN 模式测试。
- [ ] iOS / Android 真机分别通过本地 HTTP 代理测试。
- [ ] WSS 经过调试代理时，可以通过配置自定义 CA 或关闭证书校验完成测试。

## 相关资料

- `docs/research/expandation/02-network-env.md` 已列出代理、VPN、split tunnel、DNS 泄露等未覆盖场景。
- `docs/issues/dart-ffi-dns-config-gap.md` 记录过 Dart / FFI DNS 能力对齐问题。
- `docs/issues/catcher-napi-dns-cache-not-working.md` 记录过 NAPI DNS 缓存问题。
- `../echoo-flutter/weak-network/catcher-vs-legacy-ab-test-report.md` 记录了移动端弱网 A/B 测试结论。
- `../klip-electron/docs/issues/im-websocket-napi-reconnect-heartbeat-conflict.md` 记录了 NAPI WebSocket 和应用层职责边界。
