# 阶段四：软件运行环境调研

> 调研日期：2025-07-18
> 范围：操作系统、运行时、容器、浏览器引擎

---

## D1. 操作系统差异

### D1.1 TLS 库差异

| 平台 | 默认 TLS | catcher 配置 | 风险 |
|------|---------|-------------|------|
| Linux | rustls (via `rustls-tls` feature) | default | 无系统 CA 证书→需 `webpki-roots` |
| Linux | native-tls (OpenSSL) | alternative | OpenSSL 版本差异 (1.1 vs 3.x) |
| macOS | Security.framework | native-tls | 钥匙串访问权限 |
| Windows | SChannel | native-tls | 企业 CA 在 Windows 证书存储中自动可用 |
| Android | OpenSSL | native-tls/rustls | NDK 内置 OpenSSL 版本旧 |

| 场景 | 建议 |
|------|------|
| Linux + rustls + `webpki-roots` 无企业 CA | 验证 `ca_cert_pem` 手动注入 |
| macOS keychain 权限拒绝 | 验证错误信息清晰 |
| Windows SChannel 证书验证失败 | 错误映射到 `TlsError` 变体 |

### D1.2 文件系统与路径

| 场景 | 建议 |
|------|------|
| `ca_cert_path` 使用 Windows 路径 `C:\certs\ca.pem` | 验证路径解析 |
| `ca_cert_path` 指向不存在的文件 | 验证错误信息 |
| Unix domain socket 路径 (如 Docker socket) | 超出范围（HTTP 客户端不支持 UDS） |

### D1.3 进程与信号

| 场景 | 建议 |
|------|------|
| SIGTERM/SIGINT 优雅关闭 | cancelAll + destroy 应在信号处理中调用 |
| 多进程 fork 后连接池状态 | 验证 fork 后不重用父进程的连接 |

---

## D2. 浏览器引擎差异

### D2.1 JS 引擎

| 引擎 | 环境 | catcher-web 测试建议 |
|------|------|---------------------|
| V8 (Chrome, Edge, Node) | 主流 | ✅ (vitest via Node) |
| JavaScriptCore (Safari, WKWebView) | iOS/macOS | ❌ 无测试 |
| SpiderMonkey (Firefox) | 桌面 | ❌ 无测试 |

### D2.2 WebView 特殊行为

| 场景 | catcher-web 覆盖 | 建议 |
|------|:--------------:|------|
| WKWebView 第三方 Cookie 阻止 | ❌ | `credentials: 'include'` 可能被静默忽略 [1] |
| WKWebView CORS 严格模式 | ⚠️ | CORS 头必须完整 |
| Android WebView Cookie 同步 | ❌ | CookieManager 需 flush |
| Web Worker 中 `fetch` | ❌ | catcher-web 能否在 Worker 中使用 |

**关键发现**：iOS WKWebView 自 iOS 13 起在跨域请求中**静默丢弃 cookie 和 credentials**（[WebKit Bug #200857](https://bugs.webkit.org/show_bug.cgi?id=200857)），即使代码正确设置了 `credentials: 'include'`。

---

## D3. 容器与沙箱

### D3.1 Docker

| 场景 | 建议 |
|------|------|
| Alpine Linux (musl libc) | musl 的 DNS 解析行为与 glibc 不同（无 `search domains` 自动补全）[2] |
| Docker DNS (127.0.0.11) | 内置 DNS 有并发限制（~30 并发查询）[3] |
| Docker 网络模式 (bridge/host/none) | host 模式下 `localhost` 可达宿主机服务 |
| Docker Compose 服务名解析 | DNS 名如 `http://api:8080` 不含 `.`，非 FQDN |

### D3.2 沙箱环境

| 环境 | 限制 | catcher 影响 |
|------|------|-------------|
| macOS App Sandbox | 网络客户端权限 (`com.apple.security.network.client`) | 需 entitlement |
| AWS Lambda | `/tmp` 只写，无持久 TCP | 连接池无意义 |
| Cloudflare Workers | 基于 V8 Isolate，非 Node.js | 不兼容 napi-rs |
| Deno | 默认沙箱，需 `--allow-net` | catcher-http-ts 部分兼容 |

---

## 阶段四总结：关键缺失

1. **Alpine/musl DNS** — DNS 超时和重试行为差异
2. **WKWebView cookie 阻止** — 文档警告
3. **Docker DNS 并发限制** — 127.0.0.11 在大量并发 DNS 时返回错误
4. **macOS App Sandbox** — 网络客户端 entitlement 文档
5. **多进程 fork 安全** — 连接池在 fork 后的行为

---

## 引用来源

1. WebKit Bugzilla #200857, "WKWebView does not include cookies/credentials in cross-origin requests," https://bugs.webkit.org/show_bug.cgi?id=200857
2. BellSoft, "Solving DNS issues in musl," https://bell-sw.com/blog/how-to-deal-with-alpine-dns-issues/ ; and "Why Lowering ndots Breaks Alpine Pods (But Not Debian)," https://dev.to/bianbbc87/why-lowering-ndots-breaks-alpine-pods-but-not-debian-a-deep-dive-into-glibc-vs-musl-resolvers-1lhb
3. Docker, "Networking overview — embedded DNS server (127.0.0.11)," https://docs.docker.com/engine/network/
