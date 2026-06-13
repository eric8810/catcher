# 代理 / VPN / 本地网络兼容调研

**状态**: 已调研，核心自动测试已补，仍需真机验证和接入约束

**更新时间**: 2026-06-13

**关联问题**:

- [026-mobile-proxy-vpn-clash.md](./026-mobile-proxy-vpn-clash.md)
- [04-transport.md](../arch-rs/04-transport.md)
- [11-testing.md](../arch-rs/11-testing.md)

## 结论

Catcher 要兼容的不是某一种 Clash 设置，而是四类真实网络情况：

1. 用户或企业配置了显式代理：HTTP、HTTPS、SOCKS5、SOCKS5 远端 DNS。
2. 系统或浏览器按 URL 动态选择代理：PAC、WPAD、`no_proxy`、直连规则。
3. VPN / TUN 改变了系统路由和 DNS：Clash、企业 VPN、调试代理、DNS 代理。
4. 本地网络本身不同：Android Private DNS、iOS / Android VPN DNS、IPv6-only、NAT64、企业证书、登录型 Wi-Fi。

回到 Catcher，核心边界应该是：

- Catcher 负责稳定执行已经传入的 `proxy`、`dns`、`tls`、`network_path_id`。
- 外部集成负责发现当前平台应该走代理还是直连。
- PAC / 系统代理发现不应该放进 Rust 内核。
- 代理路径上，目标域名不能被 Catcher DNS 提前解析成 IP。
- 网络变化不等于配置变化。`network_changed()` 只适合同一份配置下重建连接；如果代理、DNS、证书配置变了，应重建 client。

## 调研来源

| 主题 | 来源 | 关键信息 |
|------|------|----------|
| iOS / macOS 系统代理 | [Apple CFNetwork proxy settings](https://developer.apple.com/documentation/cfnetwork/global-proxy-settings-constants)、[CFNetworkCopySystemProxySettings](https://developer.apple.com/documentation/cfnetwork/cfnetworkcopysystemproxysettings%28%29)、[Proxy Types](https://developer.apple.com/documentation/cfnetwork/proxy-types) | 系统代理可以包含 HTTP、HTTPS、SOCKS、PAC 等信息 |
| iOS URLSession 代理 | [URLSessionConfiguration.connectionProxyDictionary](https://developer.apple.com/documentation/foundation/urlsessionconfiguration/connectionproxydictionary)、[URLSession](https://developer.apple.com/documentation/foundation/urlsession) | URLSession 支持代理，但 Catcher 使用 Rust socket / reqwest，不能自动继承 URLSession 配置 |
| iOS Network.framework | [NWParameters.preferNoProxies](https://developer.apple.com/documentation/network/nwparameters/prefernoproxies)、[ProxyConfiguration](https://developer.apple.com/documentation/network/proxyconfiguration)、[NWParameters.PrivacyContext](https://developer.apple.com/documentation/network/nwparameters/privacycontext) | Apple 新 API 可以表达 HTTP CONNECT、SOCKSv5 等代理，但这属于平台接入层能力 |
| Android 网络状态 | [ConnectivityManager](https://developer.android.com/reference/android/net/ConnectivityManager)、[LinkProperties](https://developer.android.com/reference/android/net/LinkProperties) | Android 能拿默认 HTTP 代理、LinkProperties、DNS server、Private DNS、NAT64 prefix |
| Android VPN | [VpnService](https://developer.android.com/reference/kotlin/android/net/VpnService) | VPN 通常创建虚拟网络接口并改变路由，不一定暴露 HTTP/SOCKS 代理端口 |
| Android DNS 安全 | [Insecure DNS Setup](https://developer.android.com/privacy-and-security/risks/bad-dns) | 自定义 DNS 可能绕过 Android 9+ 的系统 DNS 保护；Android 建议让系统处理 DNS |
| Android 证书 | [Network Security Configuration](https://developer.android.com/privacy-and-security/security-config) | 企业证书、调试证书、用户证书需要平台侧配置，Catcher 也要支持传入 CA |
| Electron / Chromium 代理 | [Electron session](https://www.electronjs.org/docs/latest/api/session)、[Chromium proxy docs](https://chromium.googlesource.com/chromium/src/+/HEAD/net/docs/proxy.md) | Electron 可用 `session.resolveProxy(url)` 按 URL 得到代理结果；代理配置变化后要关闭旧连接 |
| SOCKS5 协议 | [RFC 1928](https://datatracker.ietf.org/doc/html/rfc1928) | SOCKS5 CONNECT 可以携带 IPv4、域名、IPv6；域名地址类型是 `0x03` |
| HTTP 代理 / CONNECT | [RFC 9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP 代理、网关、隧道是标准 HTTP 中间节点；HTTPS / WSS 常用 CONNECT 建隧道 |
| reqwest 代理能力 | [reqwest Proxy](https://docs.rs/reqwest/latest/reqwest/struct.Proxy.html)、[reqwest NoProxy](https://docs.rs/reqwest/latest/reqwest/struct.NoProxy.html) | reqwest 支持多个 Proxy 规则、SOCKS feature、Basic auth、NoProxy |
| IPv6 / NAT64 | [Apple IPv6-only Networks](https://developer.apple.com/support/ipv6/) | iOS App 必须支持 IPv6-only；硬编码 IPv4 地址会出问题 |

## 场景清单

### 1. HTTP 代理

常见形式：

```text
http://127.0.0.1:7890
http://proxy.company.com:8080
```

场景：

- Clash、Surge、Charles、Proxyman、mitmproxy 本地 HTTP 代理。
- 企业网络强制出网代理。
- Electron / 桌面端系统代理。

Catcher 当前情况：

- HTTP 和 WS 都通过 reqwest 配置 `Proxy::all(...)`。
- `ProxyConfig.auth` 支持 Basic auth。
- 已有本机代理测试：`catcher-http/tests/local_proxy_test.rs`、`catcher-ws/tests/local_proxy_test.rs`。
- 已有自动 CONNECT 探针测试，确认 HTTPS / WSS 通过 HTTP proxy 时，代理收到的是域名。

建议：

- 保持当前实现。
- 继续保留本机 Clash / 代理手动验证。

### 2. HTTPS 代理

常见形式：

```text
https://proxy.company.com:443
```

场景：

- 企业环境中代理本身也需要 TLS。
- 部分 Chromium / Electron 配置或 PAC 可以返回 HTTPS 代理。

调研结论：

- Chromium 文档说明 HTTPS 代理存在，但不是所有 HTTP 栈都稳定支持，系统代理里也不一定能直接表达。
- reqwest 类型上支持 HTTPS proxy URL，但当前 Catcher 没有单独自动化测试。

建议：

- 先作为兼容能力保留，不承诺所有企业代理认证方式都支持。
- 增加 HTTPS proxy 手动验证项。
- 如果真实客户需要，再补 HTTPS proxy 本地测试服务器。

### 3. SOCKS5 / SOCKS5 远端 DNS

常见形式：

```text
socks5://127.0.0.1:7890
socks5h://127.0.0.1:7890
```

场景：

- Clash 本地 SOCKS 端口。
- SSH 动态端口转发。
- Tor、企业 SOCKS 代理。

调研结论：

- SOCKS5 协议允许 CONNECT 请求携带域名。
- 在 Clash fake-ip、远端 DNS、按域名分流场景下，目标域名应交给代理。

Catcher 当前情况：

- `ProxyConfig::transport_url()` 会把 `socks5://` 改成 `socks5h://`。
- HTTP 和 WS 都用这个处理后的 URL。
- 已补自动测试：
  - `catcher-http/tests/proxy_dns_behavior_test.rs`
  - `catcher-ws/tests/proxy_dns_behavior_test.rs`

建议：

- 保持现在的做法。
- 不要再退回“使用系统 DNS 避开 Catcher DNS”的方案。正确做法是让代理路径不提前解析目标域名。

### 4. no_proxy / bypass / escape

常见形式：

```text
NO_PROXY=localhost,127.0.0.1,.company.internal,10.0.0.0/8
```

场景：

- 内网域名直连。
- localhost 不走代理。
- Clash escape 让某些 App、域名或地址直连。
- 企业代理排除私有网段。

调研结论：

- reqwest 有 `NoProxy`，支持域名、IPv4、IPv6、CIDR、`*`。
- Android 的 `LinkProperties.getHttpProxy()` 返回的是推荐代理，不会强制应用必须用。
- Clash 自己的 escape 规则不会自动进入 Catcher，外部集成必须把结果转成 `proxy` 或 `no_proxy`。

Catcher 当前情况：

- `ProxyConfig.no_proxy` 已有。
- HTTP 和 WS 都把 `no_proxy` 传给 reqwest。
- 已有自动测试确认 HTTP / WS 命中 `no_proxy` 时不连接代理。

建议：

- 保持 HTTP / WS 的 `no_proxy` 自动测试。
- 文档明确：外部集成必须把平台 bypass / PAC / escape 结果转换成 Catcher 配置。

### 5. PAC / WPAD

常见形式：

```text
FindProxyForURL(url, host) {
  if (dnsDomainIs(host, ".internal")) return "DIRECT";
  return "PROXY proxy.company.com:8080; DIRECT";
}
```

场景：

- 企业电脑和企业手机。
- Electron 桌面端。
- Wi-Fi 自动代理配置。

调研结论：

- PAC 是按 URL 动态返回 `DIRECT`、`PROXY`、`SOCKS` 等结果。
- Chromium / Electron 已经有成熟的代理解析能力。
- 在 Electron 中，应该用 `session.resolveProxy(url)` 得到某个 URL 的结果。

Catcher 当前情况：

- Rust 内核没有 PAC 解析器。
- `ProxyConfig` 是 client 级配置，不是每个请求动态解析。

建议：

- 不在 Catcher Rust 内核实现 PAC 解释器。
- `klip-electron` 应用 Electron 的 `session.resolveProxy(url)` 解析目标 URL。
- 对固定 base URL，可以解析一次后创建对应 Catcher client。
- 如果同一个 Catcher client 要请求多个不同域名，且 PAC 对这些域名返回不同代理，就应拆成多个 client，或后续增加“按请求选择代理”的能力。

### 6. VPN / TUN

场景：

- Clash / Stash / Shadowrocket / Surge 的 VPN 模式。
- 企业 VPN。
- Android `VpnService`。
- iOS Network Extension Packet Tunnel。

调研结论：

- VPN/TUN 改的是系统路由和 DNS，不一定有 HTTP/SOCKS 端口。
- 应用里的 socket 通常仍会被系统网络栈路由进 VPN。
- 问题常出在库自己缓存了旧 DNS、旧连接池，或者绕过了系统代理发现。

Catcher 当前情况：

- HTTP 有 `network_changed()`：清 DNS、重建 reqwest client、重置熔断器。
- WS 有 `network_changed()`：断开、重建 reqwest client、立即重连。
- `network_path_id` 已暴露给配置，但 Rust 内核目前只保存，不用它自动判断变化。

建议：

- 外部集成必须监听 VPN、Wi-Fi、蜂窝、代理、DNS 变化。
- 如果只是同一配置下网络变了，调用 `network_changed()`。
- 如果代理 URL、DNS nameserver、TLS CA、bypass 规则变了，必须重建 Catcher client。
- 可以增加诊断日志，打印 `network_path_id`、DNS mode、proxy scheme、是否命中 no_proxy。

### 7. DNS：系统 DNS、Private DNS、fake-ip、自定义 nameserver

场景：

- Android Private DNS。
- iOS VPN DNS。
- Clash fake-ip / redir-host。
- 企业 split DNS。
- 内网域名。
- Catcher 自定义 `nameservers` 和 `host_mapping`。

调研结论：

- Android 文档提示，自定义 DNS 可能绕过 Android 9+ 的系统 DNS 保护。
- Android `LinkProperties` 能看到 DNS server、Private DNS、NAT64 prefix。
- 代理路径下，目标域名应该交给代理解析。

Catcher 当前情况：

- `dns` 配置存在时，默认仍是 `mode = "catcher"`。
- `dns.mode = "native"` 是显式退出 Catcher DNS。
- `fallback_to_default_nameservers` 默认关闭。
- 代理路径已经避免提前解析目标域名。

建议：

- 保持 Catcher DNS 默认能力，不把默认改回 native。
- 外部集成如果检测到 Android strict Private DNS，又没有显式业务需求使用 Catcher DNS，应考虑传 `dns.mode = "native"` 或不传 `dns`。
- 如果业务需要 Catcher DNS，就应明确传入 nameserver / host_mapping，并在网络变化时重建。
- 补文档说明：host_mapping 到 IPv4 地址可能破坏 IPv6-only / NAT64 场景。

### 8. TLS / 企业证书 / 调试证书

场景：

- Charles、Proxyman、mitmproxy 抓包。
- 企业网关 HTTPS 检查。
- 自签名内网证书。

调研结论：

- Android 官方支持 Network Security Configuration 配置自定义 CA、debug CA、用户证书。
- iOS / macOS 也有系统证书信任链和 URLSession / Network.framework 的 TLS 配置能力。

Catcher 当前情况：

- HTTP 和 WS 都有 `TlsConfig`。
- 支持 `reject_unauthorized`、自定义 CA、客户端证书等字段。
- WS 的 `pin_sha256` 当前仍未实现。

建议：

- 外部集成必须把企业 CA 或调试 CA 转成 Catcher 可用的 `TlsConfig`。
- 文档中明确：如果系统 URLSession 能访问，但 Catcher 不能访问，除了 proxy/DNS，还要检查 CA 是否传入 Rust 层。
- 补 WS `pin_sha256` 的状态说明，避免误以为 HTTP/WS 完全一致。

### 9. IPv6-only / NAT64

场景：

- iOS IPv6-only 审核环境。
- 运营商 IPv6-only 或 IPv6 优先网络。
- NAT64 / DNS64。

调研结论：

- Apple 明确要求 App 支持 IPv6-only。
- 硬编码 IPv4、只走 IPv4 socket、把域名映射到 IPv4 literal 都可能失败。

Catcher 当前情况：

- reqwest / rustls 正常支持 IPv6。
- `host_mapping` 可以写 IPv4 或 IPv6，但如果移动网络是 IPv6-only，映射到 IPv4 可能破坏 NAT64。

建议：

- 文档中要求移动端不要把公网域名长期 host_mapping 到 IPv4。
- 增加 IPv6 / NAT64 手动验证项。
- 自动测试可以补 IPv6 loopback，但它不能完全代表 NAT64。

### 10. 环境变量代理

常见形式：

```bash
HTTP_PROXY=http://127.0.0.1:7890
HTTPS_PROXY=http://127.0.0.1:7890
ALL_PROXY=socks5://127.0.0.1:7890
NO_PROXY=localhost,127.0.0.1
```

场景：

- Node.js 服务端。
- CLI 工具。
- 开发机本地代理。

Catcher 当前情况：

- 纯 TS 包有 `proxy: true` 读取环境变量的逻辑。
- Rust / NAPI / Dart 层不自动读环境变量。

建议：

- 保持 Rust 内核不读环境变量，避免移动端行为不可控。
- Node / Electron 接入层可以读取环境变量，并显式传 `ProxyConfig`。
- 如果将来需要，可在 NAPI TS wrapper 增加“读取环境变量并转成 config”的辅助函数，不放进 Rust core。

### 11. 登录型 Wi-Fi / captive portal

场景：

- 酒店、机场、公司访客 Wi-Fi。
- DNS 可解析，但 HTTP/HTTPS 被重定向或阻断，直到用户登录。

Catcher 当前情况：

- 没有专门识别 captive portal。

建议：

- 不在 Catcher 内核做网页登录判断。
- 错误分类和诊断日志要能区分 DNS、连接、TLS、HTTP 3xx/4xx。
- 外部 App 自己决定是否打开系统网页登录页或提示用户。

## Catcher 架构调整建议

### 已经正确的方向

1. **Catcher DNS 不能被取消。**
   DNS 缓存、host mapping、自定义 nameserver 仍是 Catcher 的核心能力。

2. **代理路径不能提前解析业务域名。**
   `socks5://` 按 `socks5h://` 处理是正确方向。HTTP 和 WS 都已覆盖。

3. **HTTP 和 WS 统一走 reqwest 配置。**
   这样 proxy、TLS、DNS、连接池重建逻辑一致。

4. **网络变化要打断旧连接。**
   HTTP 重建 reqwest client；WS 断开并立即重连。

### 应该补的代码和测试

| 优先级 | 项目 | 原因 |
|--------|------|------|
| P0 | 诊断日志 | 客户现场最难判断卡在 DNS、代理连接、TLS 还是 WS 握手 |
| P1 | 明确 `network_changed()` vs 重建 client | 配置变化时不能只调用 `network_changed()` |
| P1 | Electron 接入文档 | `session.resolveProxy(url)` 结果要转换成 `ProxyConfig` 或直连 |
| P1 | Android / iOS 接入文档 | 平台 DNS、代理、VPN、证书变化如何映射到 Catcher |
| P2 | HTTPS proxy 测试 | 企业场景可能需要，但系统代理里不一定常见 |
| P2 | IPv6 / NAT64 验证 | 移动端必须列入发版前真机验证 |
| P2 | 代理认证扩展 | 目前只有 Basic；NTLM / Kerberos 等企业代理另行评估 |

### 不应该现在做的事

1. **不要在 Rust 内核实现 PAC 解释器。**
   PAC 是平台和浏览器已有能力，Electron 和系统 API 更适合做。

2. **不要让 Catcher 自动猜系统代理。**
   移动端权限、系统差异、PAC、VPN、企业配置都太多。自动猜错比不猜更危险。

3. **不要把 DNS 默认改成 native。**
   这会削弱 Catcher 的价值。正确做法是：代理路径交给代理解析，非代理路径继续保留 Catcher DNS 能力。

4. **不要把所有网络变化都当成同一种变化。**
   网络切换、代理变化、DNS 变化、证书变化的处理不同。配置变了就重建 client。

## 对外部集成的明确要求

### echoo-flutter

需要做：

- iOS / Android 侧发现当前是否有 VPN。
- Android 读取 `ConnectivityManager`、`LinkProperties`、默认 proxy、DNS server、Private DNS。
- iOS 读取可用的系统代理信息；如果平台无法稳定读取，则由 App 配置层显式传入。
- 代理、DNS、证书、网络路径变化后重建 HTTP / WS client。
- 如果只是同配置网络切换，调用 `networkChanged()`。

不能做：

- 不能假设 Dart 原有 HTTP client 能用，Rust FFI 就自动继承同样代理。
- 不能只给 HTTP 传 proxy，而 WS 不传。

### klip-electron

需要做：

- 对每个 base URL 使用 `session.resolveProxy(url)`。
- 解析结果为 `DIRECT` 时，不传 proxy。
- 解析结果为 `PROXY host:port` 时，传 `http://host:port`。
- 解析结果为 `HTTPS host:port` 时，传 `https://host:port` 并列入验证。
- 解析结果为 `SOCKS` / `SOCKS5` 时，传 `socks5://host:port`，由 Catcher 内部转成远端 DNS。
- 代理配置变化后关闭旧连接并重建 Catcher client。

不能做：

- 不要把 PAC 文件路径直接传给 Catcher。
- 不要让一个 Catcher client 混跑多个 PAC 结果不同的域名。

## 验证矩阵

| 场景 | 自动测试 | 手动测试 |
|------|----------|----------|
| HTTP proxy | 已有本机 ignored 测试 | Clash HTTP proxy |
| HTTP CONNECT | 已有自动测试 | HTTPS / WSS 过本地 HTTP proxy |
| SOCKS5 远端 DNS | 已有自动测试 | Clash SOCKS5 |
| no_proxy / bypass | 已有自动测试 | Clash escape / 企业 bypass |
| PAC | 不在 Rust 内核测 | Electron `session.resolveProxy(url)` |
| VPN/TUN | 部分通过 `network_changed()` 测试 | iOS / Android 真机 Clash VPN |
| Android Private DNS | 无 | Android 真机 |
| 企业 CA / 抓包证书 | 部分 TLS 单测 | Charles / Proxyman / mitmproxy |
| IPv6 / NAT64 | 待补 IPv6 loopback | iOS IPv6-only / NAT64 |
| captive portal | 不测 | 真实 Wi-Fi |

## 下一步

1. 增加安全诊断日志，不记录敏感 token，只记录网络路径、DNS 模式、proxy scheme、错误阶段。
2. 把 echoo-flutter 和 klip-electron 的接入要求更新到各自 `docs/issues/`。
3. 在真机上跑 Clash VPN / fake-ip / escape / HTTP proxy / SOCKS5 / Private DNS / IPv6 的验证矩阵。
