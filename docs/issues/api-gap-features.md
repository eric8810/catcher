# API Gap — 功能补充 Issues

> 来源：[api-gap-analysis.md](../research/api-gap-analysis.md) 逐项对照 + 代码审查确认
> 日期：2026-06-12

## 问题清单

| # | Issue | 优先级 | 影响范围 | 状态 |
|---|-------|:------:|----------|:----:|
| G2 | 错误上下文丰富化 | 🔴 P0 | TS + Rust | ✅ |
| G3 | CORS / credentials / cookie 管理 | 🔴 P0 | TS (web + http-ts) | ✅ |
| G4 | 代理设置 (HTTP/SOCKS5) | 🟡 P1 | TS + Rust + FFI | ✅ |
| G5 | FormData / 文件上传 | 🟡 P1 | TS + Rust + FFI | 🟡 TS✅ Rust❌ |
| G6 | 重定向控制 | 🟡 P1 | TS + Rust | ✅ |
| G7 | 自定义 Hostname 解析 (host_mapping) | 🟡 P1 | Rust + TS + FFI | ✅ |
| G8 | HTTPS 配置增强 (mTLS / SNI / pinning) | 🟡 P1 | Rust + TS + FFI | 🟡 mTLS✅ SNI✅ TLS版本✅ pin_sha256❌ |
| G9 | Transport trait (自定义 Adapter) | 🟡 P1 | Rust + TS | 🔲 |
| G10 | 流式响应 (Response Stream) | 🟡 P1 | TS + Rust | ✅ |
| G11 | 韧性运行时控制 | 🟡 P1 | TS + Rust + FFI | ✅ |
| G12 | 认证辅助 (Basic Auth / Bearer / XSRF) | 🟢 P2 | TS | ✅ |

> ~~G1 (AbortSignal) 已实现~~：catcher-http-ts (client.ts:232-237, :243) 和 catcher-web (client.ts:56-76, :148-152, :169) 均已完整支持，含重试时取消不重试。
>
> G3 (CORS/cookie) 和 G4 (proxy) 为代码审查中新发现，api-gap-analysis.md 未涉及。

---

## Issue 之间依赖关系

```
G2 (错误上下文) ─ 独立
G3 (CORS/cookie) ── 独立，仅影响 TS 层
G4 (proxy) ────── 依赖 G9 (Transport trait 可选，但不阻塞)
G5 (FormData) ──── 独立
G6 (重定向) ────── 独立
G7 (host_mapping) ─ 与 G8 (SNI 覆盖) 有关联
G8 (HTTPS 增强) ── 与 G7 有关联
G9 (Transport) ──── 独立，但其他功能可基于此扩展
G10 (流式响应) ──── 与 G9 (Transport trait) 协同
G11 (韧性控制) ──── 独立
G12 (认证) ──────── 与 G3 (cookie) 有关联（XSRF 依赖 cookie）
```

---

## G2: 错误上下文丰富化

**优先级**: 🔴 P0
**影响范围**: `catcher-http-ts`, `catcher-web`, `catcher-http` (Rust)

### 现状

- `CatcherError` 枚举仅包含错误类型（Timeout / Connection / Dns / Tls / ...）
- 错误中无法回溯原始请求 URL、method、headers、config
- 重试错误不携带第几次重试信息
- 响应错误中 `body` 是 String 而非原始字节

### 目标

```typescript
interface CatcherHttpError extends Error {
  type: 'timeout' | 'connection' | 'dns' | 'tls' | 'http' | 'cancelled' | 'unknown'
  request: {
    method: string
    url: string
    headers: Record<string, string>
    config: RequestConfig
  }
  response?: {
    status: number
    headers: Record<string, string>
    data: unknown        // 尝试解析后的响应体
    rawData: Uint8Array  // 原始字节
  }
  attempt: number        // 第几次重试失败（0-based）
  elapsedMs: number
  toJSON(): object       // 序列化（可脱敏）
}
```

### 验收标准

- [ ] catch 到错误后可访问 `error.request.url`、`error.request.method`
- [ ] HTTP 错误可访问 `error.response.status`、`error.response.data`
- [ ] 重试失败后 `error.attempt` 反映实际重试次数
- [ ] `error.toJSON()` 可安全打印（不含敏感 header 如 Authorization）

---

## G3: CORS / Credentials / Cookie 管理

**优先级**: 🔴 P0
**影响范围**: `catcher-web`, `catcher-http-ts`, `catcher-ws-ts`, `catcher-core-ts`

### 现状

所有源码中以下关键词零匹配：`cors`, `credential`, `cookie`, `withCredentials`, `Access-Control`, `same-origin`, `xsrf`, `csrf`。

**catcher-web (浏览器)**:
- `fetch()` 调用未传 `credentials` 和 `mode` 参数
- 默认行为：`credentials: 'same-origin'`，跨域请求不会携带 cookie
- 无法配置为 `credentials: 'include'`（跨域带 cookie 的标准做法）

**catcher-http-ts (Node.js)**:
- axios 未配置 `withCredentials`
- 无 cookie jar 管理（可选依赖 `tough-cookie` 未集成）

**catcher-ws-ts (Node.js)**:
- WS 连接的 cookie header 传递未显式支持

### 目标

**1. 共享类型 (`catcher-core-ts`)**:
```typescript
interface HttpClientConfig {
  // ...existing
  /** 浏览器端 fetch credentials 策略 */
  credentials?: 'include' | 'same-origin' | 'omit'
  /** 浏览器端 fetch mode */
  fetchMode?: 'cors' | 'no-cors' | 'same-origin' | 'navigate'
  /** Node.js 端 axios withCredentials */
  withCredentials?: boolean
}

interface RequestConfig {
  // ...existing
  /** 请求级 credentials 覆盖 */
  credentials?: 'include' | 'same-origin' | 'omit'
}
```

**2. catcher-web**: 传递 `credentials` 和 `mode` 到 `fetch()` 调用

**3. catcher-http-ts**: 
- 传递 `withCredentials` 到 axios
- 可选: 集成 `tough-cookie` 实现 cookie jar

**4. catcher-ws-ts**:
- WS 连接配置中支持 `headers.cookie` 或 `cookie` 便捷字段

### 验收标准

- [ ] `createHttpClient({ baseURL, credentials: 'include' })` 跨域请求携带 cookie
- [ ] `client.get(url, { credentials: 'omit' })` 请求级覆盖
- [ ] catcher-web 的 `fetch` 调用正确传递 `credentials` 和 `mode`
- [ ] 文档说明 WebSocket 跨域 cookie 的限制（SameSite=None; Secure）

---

## G4: 代理设置 (HTTP/HTTPS/SOCKS5 Proxy)

**优先级**: 🟡 P1
**影响范围**: `catcher-http-ts`, `catcher-ws-ts`, `catcher-http` (Rust), FFI

### 现状

- 所有源码中无 proxy 配置（文档中的 proxy 均为测试用的网络损伤模拟代理）
- Rust reqwest 支持 `.proxy()` 但 FFI 未暴露
- axios 支持通过 `httpAgent` / `httpsAgent` 配置代理
- `ws` 库支持通过 `agent` 参数配置代理

### 目标

**共享类型**:
```typescript
interface ProxyConfig {
  /** 代理 URL: "http://host:port" | "https://host:port" | "socks5://host:port" */
  url: string
  /** 代理认证 */
  auth?: { username: string; password: string }
  /** 不走代理的 hostname 列表 */
  noProxy?: string[]
}

interface HttpClientConfig {
  // ...existing
  /** 代理配置，true = 自动读取环境变量 HTTP_PROXY/HTTPS_PROXY/NO_PROXY */
  proxy?: boolean | string | ProxyConfig
}
```

**catcher-http-ts**: 使用 `https-proxy-agent` / `socks-proxy-agent` 创建代理 Agent
**catcher-ws-ts**: WS 连接传入代理 Agent
**catcher-http (Rust)**: 暴露 reqwest 的 `.proxy()` 配置
**FFI**: 增加 proxy 相关配置字段

### 验收标准

- [ ] `createHttpClient({ baseURL, proxy: 'http://proxy:8080' })` 所有请求走代理
- [ ] `proxy: true` 自动读取环境变量
- [ ] 支持 SOCKS5 代理
- [ ] `noProxy` 列表中的 host 绕过代理
- [ ] WS 连接也支持代理

---

## G5: FormData / 文件上传

> **原生层设计**：[native-layer-capability-gaps.md](./native-layer-capability-gaps.md) N-01 — Rust 原生层 multipart 编码器 + FFI 导出方案

**优先级**: 🟡 P1
**影响范围**: `catcher-http-ts`, `catcher-web`, `catcher-http` (Rust), FFI

### 现状

完全缺失。无 FormData 构建、无 multipart/form-data 自动处理、无文件上传 API。

### 目标

- TS 层: 利用浏览器原生 `FormData`（web）或 `form-data` npm 包（Node.js）
- 自动设置 `Content-Type: multipart/form-data`
- 提供便捷方法 `client.upload(url, formData, config?)` 或在 `post` 中自动检测 FormData
- Rust 层: 使用 `reqwest::multipart::Form`

### 验收标准

- [ ] Node.js: `client.post(url, formData)` 自动处理 multipart
- [ ] Browser: `client.post(url, formData)` 使用原生 FormData
- [ ] 支持文件流上传（`createReadStream`）
- [ ] 与进度回调 `onUploadProgress` 配合使用

---

## G6: 重定向控制

**优先级**: 🟡 P1
**影响范围**: `catcher-http-ts`, `catcher-web`, `catcher-http` (Rust)

### 现状

无任何重定向控制。Rust reqwest 默认跟随重定向（最多 10 次），axios 在 Node.js 默认跟随最多 5 次，浏览器 fetch 默认跟随。用户无法：
- 禁止跟随重定向
- 限制最大重定向次数
- 在重定向前拦截检查

### 目标

```typescript
interface HttpClientConfig {
  // ...existing
  redirect?: {
    follow?: boolean       // 是否跟随，默认 true
    maxRedirects?: number  // 最大次数，默认 5
    beforeRedirect?: (info: { url: string; status: number; headers: Record<string, string> }) => boolean
  }
}
```

### 验收标准

- [ ] `redirect: { follow: false }` 返回 3xx 响应体而非跟随
- [ ] `redirect: { maxRedirects: 0 }` 等效于 `follow: false`
- [ ] `beforeRedirect` 返回 `false` 时停止跟随
- [ ] 超过 `maxRedirects` 时抛出 `MaxRedirectError`

---

## G7: 自定义 Hostname 解析 (host_mapping)

**优先级**: 🟡 P1
**影响范围**: `catcher-http` (Rust), `catcher-http-ts`, FFI

### 现状

- Rust `DnsConfig` 仅有缓存参数
- 无 hostname → IP 直映射能力
- 企业内网、灰度发布、开发调试等场景需要

### 目标

```typescript
interface DnsConfig {
  // ...existing (dnsCacheTtl, etc.)
  /** 自定义 DNS 服务器 */
  nameservers?: string[]
  /** hostname → IP 直映射，绕过 DNS 解析 */
  hostMapping?: Record<string, string>
}
```

Rust 层: 实现 `Resolve` trait，优先查 `host_mapping`，未命中走 nameservers 或系统 DNS。TLS SNI 保持原始 hostname。

### 验收标准

- [ ] `{ hostMapping: { 'api.example.com': '10.0.0.5' } }` 请求直连 10.0.0.5
- [ ] TLS SNI 仍使用原始 hostname（证书校验不失败）
- [ ] 请求级可覆盖 hostMapping（灰度按用户路由）

---

## G8: HTTPS 配置增强 (mTLS / SNI / pinning)

**优先级**: 🟡 P1
**影响范围**: `catcher-http` (Rust), `catcher-http-ts`, FFI

### 现状

- Rust `TlsConfig` 仅有 `reject_unauthorized` + `ca_cert_path` + `client_cert_path`
- **mTLS 不可用**：有 client cert 无 client key
- 不支持 CA 内联（仅文件路径）、DER/PFX 格式
- 无 SNI 覆盖、TLS 版本控制、证书 pinning

### 目标

```typescript
interface TlsConfig {
  rejectUnauthorized?: boolean
  // CA 证书
  caCertPath?: string
  caCertPem?: string        // 内联 PEM
  // 客户端证书 (mTLS)
  clientCertPath?: string
  clientCertPem?: string
  clientKeyPath?: string    // ← 关键缺失
  clientKeyPem?: string     // ← 关键缺失
  // 高级
  tlsSniOverride?: string   // 配合 host_mapping
  minTlsVersion?: '1.0' | '1.1' | '1.2' | '1.3'
  maxTlsVersion?: '1.0' | '1.1' | '1.2' | '1.3'
  pinSha256?: string[]      // 证书公钥指纹
}
```

### 验收标准

- [ ] mTLS: 提供客户端证书 + 私钥，完成双向 TLS 握手
- [ ] CA 内联: `caCertPem` 直接传 PEM 字符串
- [ ] SNI 覆盖: 配合 G7 host_mapping 使用时不因证书 CN 不匹配而失败
- [ ] TLS 版本: 限制最低 TLS 1.2

---

## G9: Transport trait (自定义 Adapter)

**优先级**: 🟡 P1
**影响范围**: `catcher-http` (Rust)

### 现状

传输层硬编码为 reqwest，无法替换。axios 有 `adapter`，dio 有 `HttpClientAdapter`。

### 目标

```rust
pub trait Transport: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError>;
}
```

`HttpTransport` 实现此 trait，调用方可实现 mock / 缓存 / 代理等 adapter。

### 验收标准

- [ ] 测试中可用 MockTransport 替换真实 HTTP
- [ ] 现有 `HttpTransport` 功能不受影响
- [ ] 支持在 `HttpClientConfig` 中注入自定义 Transport

---

## G10: 流式响应 (Response Stream)

> **原生层设计**：[native-layer-capability-gaps.md](./native-layer-capability-gaps.md) N-02 — Rust `execute_stream` + `catcher_http_execute_stream` C ABI 方案

**优先级**: 🟡 P1
**影响范围**: `catcher-http-ts`, `catcher-web`, `catcher-http` (Rust)

### 现状

- `responseType` 已有 `'json' | 'text' | 'bytes'`，但缺少 `'stream'`
- 大文件下载、实时数据处理等场景需要流式读取
- Rust reqwest 支持 `bytes_stream()`
- 浏览器 `fetch` 返回的 `ReadableStream` 未暴露

### 目标

```typescript
interface RequestConfig {
  responseType?: 'json' | 'text' | 'bytes' | 'stream'
}

// 流式响应
interface HttpResponse<T = any> {
  data: T | ReadableStream<Uint8Array>  // stream 模式时为 ReadableStream
}
```

### 验收标准

- [ ] `responseType: 'stream'` 返回可读流
- [ ] 与 `onDownloadProgress` 配合使用
- [ ] Node.js 返回 Node.js Readable，浏览器返回 Web ReadableStream

---

## G11: 韧性运行时控制

> **原生层设计**：[native-layer-capability-gaps.md](./native-layer-capability-gaps.md) N-03 — 单请求级 cancel + N-04 — 网络质量推送订阅

**优先级**: 🟡 P1
**影响范围**: `catcher-http-ts`, `catcher-http` (Rust), FFI

### 现状

- TS 层: `circuitBreakerState()` 和 `queueDepth()` 已暴露
- 所有韧性配置创建后不可修改
- 无事件通知（重试、熔断状态变更、网络质量变化）
- Rust 层运行时查询和事件均缺失

### 目标

1. **运行时配置热更新**: `client.updateConfig({ retry: { attempts: 5 } })`
2. **状态查询**: 已有 TS 层，Rust 层需补充
3. **事件订阅**:
```typescript
client.on('retry', (e) => { /* e.attempt, e.error */ })
client.on('circuitBreakerChange', (e) => { /* e.from, e.to */ })
client.on('networkQualityChange', (e) => { /* e.from, e.to */ })
```

### 验收标准

- [ ] 运行时修改 retry/circuitBreaker 配置立即生效
- [ ] 熔断器状态变更触发事件
- [ ] 重试时触发事件，携带 attempt 和 error 信息
- [ ] FFI 层暴露状态查询和事件回调

---

## G12: 认证辅助 (Basic Auth / Bearer / XSRF)

**优先级**: 🟢 P2
**影响范围**: `catcher-http-ts`, `catcher-web`

### 现状

- 无 HTTP Basic Auth 便捷字段
- 无 Bearer Token 自动注入
- 无 XSRF/CSRF 自动处理
- 所有认证均需用户手动设置 headers

### 目标

```typescript
interface HttpClientConfig {
  auth?: {
    username: string
    password: string
  }
  // 或者
  bearerToken?: string | (() => Promise<string>)  // 支持刷新
  xsrfCookieName?: string
  xsrfHeaderName?: string
}
```

### 验收标准

- [ ] `auth: { username, password }` 自动添加 `Authorization: Basic xxx` header
- [ ] `bearerToken` 自动添加 `Authorization: Bearer xxx` header
- [ ] `bearerToken` 为函数时支持异步刷新
- [ ] `xsrfCookieName + xsrfHeaderName` 从 cookie 读取 XSRF token 写入 header

---

## 实施顺序建议

```
Phase 1 (P0):
  G2 (错误上下文) + G3 (CORS/cookie)
  → 这两项影响基础可用性，应最先完成

Phase 2 (P1 - 独立):
  G4 (proxy) + G6 (重定向) + G5 (FormData)

Phase 3 (P1 - 关联组):
  G7 (host_mapping) + G8 (HTTPS 增强) — 互相依赖，一起做
  G9 (Transport trait) + G10 (流式响应)

Phase 4 (P1 - 韧性):
  G11 (韧性运行时控制)

Phase 5 (P2):
  G12 (认证辅助)
```
