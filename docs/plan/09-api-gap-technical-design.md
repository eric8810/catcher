# API Gap 功能补充 — 技术设计方案

> 对应 Issues: [api-gap-features.md](../issues/api-gap-features.md) G2–G12
> 调研基础：源码逐文件审查，含精确行号引用
>
> **注**：G1 (AbortSignal) 已实现 — catcher-http-ts (client.ts:232-237, :243) + catcher-web (client.ts:56-76, :148-152, :169)，含重试取消不重试。

---

## 修改总览

### 共享类型变更 (`catcher-core-ts/src/types.ts`)

以下字段需新增到现有接口：

```
HttpClientConfig (line 22):
  + credentials?: 'include' | 'same-origin' | 'omit'
  + fetchMode?: 'cors' | 'no-cors' | 'same-origin' | 'navigate'
  + withCredentials?: boolean
  + proxy?: boolean | string | ProxyConfig
  + redirect?: { follow?: boolean; maxRedirects?: number; beforeRedirect?: (...) => boolean }
  + auth?: { username: string; password: string }
  + bearerToken?: string | (() => string | Promise<string>)
  + xsrfCookieName?: string
  + xsrfHeaderName?: string

RequestConfig (line 58):
  + credentials?: 'include' | 'same-origin' | 'omit'   // 请求级覆盖
  + proxy?: boolean | string | ProxyConfig              // 请求级覆盖

新增接口:
  + ProxyConfig { url, auth?, noProxy? }
  + DnsConfig { nameservers?, hostMapping? }
  + TlsConfig (增强版)
  + RedirectInfo { url, status, headers }
  + CatcherHttpError (增强错误类型)
  + ClientEvent (韧性事件联合类型)
```

---

## G2: 错误上下文丰富化

### TS 层 — 统一错误类型

**当前状态**: `client.ts` catch 块抛出原始 Error 或 axios error，无统一结构。

**方案**: 新增 `CatcherHttpError` 类，在 `client.ts` 的 catch 块中包装所有错误。

```typescript
// catcher-core-ts/src/types.ts — 新增
export interface CatcherHttpError extends Error {
  readonly type: 'timeout' | 'connection' | 'dns' | 'tls' | 'http' | 'cancelled' | 'unknown'
  readonly request: {
    method: string
    url: string
    headers: Record<string, string>
    config: RequestConfig
  }
  readonly response?: {
    status: number
    headers: Record<string, string>
    data: unknown
    rawData?: Uint8Array
  }
  readonly attempt: number
  readonly elapsedMs: number
  toJSON(): Record<string, unknown>
}

export function isCatcherError(err: unknown): err is CatcherHttpError { ... }
```

**接入点**:

1. **catcher-http-ts `client.ts:282`** — catch 块中，根据 axios error code 分类并包装：
   - `ECONNABORTED` / `ETIMEDOUT` → `type: 'timeout'`
   - `ECONNREFUSED` / `ENOTFOUND` → `type: 'connection'` / `'dns'`
   - `UNABLE_TO_VERIFY_LEAF_SIGNATURE` → `type: 'tls'`
   - 有 response → `type: 'http'`
   - `isCancel` → `type: 'cancelled'`

2. **retry.ts:68-70** — 在 `onFailedAttempt` 中记录 attempt 数，传递到最终错误：
   ```typescript
   // retry.ts 中记录最近一次 attempt
   let lastAttempt = 0
   // onFailedAttempt 中:
   lastAttempt = error.attemptNumber
   // 最终 catch 中使用 lastAttempt 构造错误
   ```

3. **catcher-web `client.ts`** — 类似的 catch 块包装：
   - `AbortError` → `type: 'cancelled'`
   - `TypeError: Failed to fetch` → `type: 'connection'`（CORS/网络错误）
   - 有 response → `type: 'http'`

### Rust 层

```rust
// catcher-http/src/types/http.rs — 增强错误
pub struct RequestError {
    pub kind: CatcherError,
    pub request: HttpRequest,         // 原始请求回引
    pub response: Option<HttpResponse>,
    pub attempt: u32,
    pub elapsed_ms: u64,
}

impl std::fmt::Display for RequestError {
    // 脱敏输出：不打印 Authorization / Cookie 等 header
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 新增 `CatcherHttpError` 接口 + `isCatcherError` |
| `catcher-http-ts/src/http/client.ts:282` | catch 块中包装为 CatcherHttpError |
| `catcher-http-ts/src/http/retry.ts:68` | 记录 attempt 数 |
| `catcher-web/src/http/client.ts` | catch 块中包装 |
| `catcher-http/src/types/http.rs` | 新增 RequestError |
| `catcher-http/src/transport/http_client.rs` | 使用 RequestError |

---

## G3: CORS / Credentials / Cookie

### `catcher-core-ts/src/types.ts` — 类型扩展

```typescript
export interface HttpClientConfig {
  // ...existing (line 22-54)...
  /** 浏览器端 fetch credentials 策略 */
  credentials?: 'include' | 'same-origin' | 'omit'
  /** 浏览器端 fetch mode */
  fetchMode?: 'cors' | 'no-cors' | 'same-origin' | 'navigate'
  /** Node.js 端 axios withCredentials */
  withCredentials?: boolean
}

export interface RequestConfig {
  // ...existing (line 58-83)...
  /** 请求级 credentials 覆盖 */
  credentials?: 'include' | 'same-origin' | 'omit'
}
```

### `catcher-web/src/http/client.ts` — fetch 配置

**接入点**: `fetch()` 调用处构建 `RequestInit`。

```typescript
const fetchOptions: RequestInit = {
  method,
  headers: normalizedHeaders,
  body: body ?? undefined,
  credentials: merged.credentials ?? instanceConfig.credentials ?? 'same-origin',
  mode: merged.fetchMode ?? instanceConfig.fetchMode ?? 'cors',
  signal: merged.signal,
}
```

关键：`credentials: 'include'` 使跨域请求携带 cookie，`mode: 'cors'`（默认值）允许跨域。

### `catcher-http-ts/src/http/client.ts` — axios 配置

**接入点**: `client.ts:240-259` 的 `axiosConfig`。

```typescript
const axiosConfig: AxiosRequestConfig = {
  // ...existing...
  withCredentials: config.withCredentials,  // ← 新增
}
```

### `catcher-ws-ts/src/ws/client.ts` — WS Cookie

**接入点**: `client.ts` 中 `new WebSocket(url, options)` 调用处。

WS 连接在 Node.js 中通过 `ws` 库的 `headers` 选项传递 cookie：

```typescript
const wsOptions: ClientOptions = {
  // ...existing...
  headers: {
    ...options.headers,
    ...(options.cookie ? { Cookie: typeof options.cookie === 'string' ? options.cookie : undefined } : {}),
  },
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts:22-54` | HttpClientConfig 增加 credentials/fetchMode/withCredentials |
| `catcher-core-ts/src/types.ts:58-83` | RequestConfig 增加 credentials |
| `catcher-core-ts/src/types.ts:149` | ResilientWSOptions 增加 cookie 字段 |
| `catcher-web/src/http/client.ts` | fetch options 传递 credentials/mode |
| `catcher-http-ts/src/http/client.ts:240-259` | axiosConfig 传递 withCredentials |
| `catcher-ws-ts/src/ws/client.ts` | WS 连接传递 cookie header |

---

## G4: 代理设置 (HTTP/SOCKS5)

### 类型定义

```typescript
// catcher-core-ts/src/types.ts — 新增
export interface ProxyConfig {
  /** "http://host:port" | "https://host:port" | "socks5://host:port" */
  url: string
  auth?: { username: string; password: string }
  noProxy?: string[]
}

export interface HttpClientConfig {
  // ...existing...
  proxy?: boolean | string | ProxyConfig
}
```

### `catcher-http-ts` — axios 代理

**接入点**: `client.ts:129` 创建 axios instance 处。

**方案 A (推荐)**: 使用 `https-proxy-agent` + `socks-proxy-agent` 创建代理 Agent，**包装** SharedAgent 以保留 DNS 缓存和 keepAlive 能力。

```typescript
import { HttpsProxyAgent } from 'https-proxy-agent'
import { SocksProxyAgent } from 'socks-proxy-agent'

// 在 createHttpClient 中
const baseAgent = createSharedAgent({ keepAlive, dnsCacheTtl, rejectUnauthorized })

let agent = baseAgent
if (config.proxy) {
  const proxyConfig = resolveProxyConfig(config.proxy) // 处理 boolean/string/ProxyConfig
  const proxyUrl = buildProxyUrl(proxyConfig)
  // 代理 agent 包装 baseAgent，保留 DNS 缓存和 keepAlive
  agent = proxyUrl.startsWith('socks')
    ? new SocksProxyAgent(proxyUrl, { agent: baseAgent })
    : new HttpsProxyAgent(proxyUrl, { agent: baseAgent })
}

const instance = axios.create({ ...axiosDefaults, httpAgent: agent, httpsAgent: agent })
```

> **注意**：代理 agent 必须包装 SharedAgent 而非替换它，否则 DNS 缓存、keepAlive 连接池和健康检查会全部丢失。
> `https-proxy-agent` 和 `socks-proxy-agent` 均支持 `agent` 选项传入底层 agent。

**方案 B (轻量)**: 直接使用 axios 内置的 `proxy` 配置（不支持 SOCKS5）：

```typescript
const axiosConfig = {
  proxy: proxyConfig ? {
    host: parsed.host,
    port: parsed.port,
    auth: proxyConfig.auth ? { ...proxyConfig.auth } : undefined,
  } : false,
}
```

选择依据：是否需要 SOCKS5 支持。如果需要，用方案 A；否则方案 B 更轻量。

### `catcher-ws-ts` — WS 代理

```typescript
// ws 连接时传入代理 agent
const wsOptions = {
  agent: proxyAgent,  // 同上创建的 HttpsProxyAgent / SocksProxyAgent
}
const ws = new WebSocket(url, protocols, wsOptions)
```

### Rust 层

**接入点**: `catcher-http/src/transport/http_client.rs` 中 reqwest Client 构建处。

reqwest 原生支持代理：

```rust
pub fn build_client(config: &HttpClientConfig) -> Result<Client, CatcherError> {
    let mut builder = Client::builder();
    // ...existing TLS/timeout config...

    if let Some(ref proxy) = config.proxy {
        match proxy {
            ProxySetting::Auto => {
                // 读取环境变量 HTTP_PROXY / HTTPS_PROXY / NO_PROXY
                builder = builder.set_from_env(true);
            }
            ProxySetting::Url(url) => {
                let p = reqwest::Proxy::all(url)?;
                builder = builder.proxy(p);
            }
            ProxySetting::Config(cfg) => {
                let p = reqwest::Proxy::all(&cfg.url)?;
                if let Some(ref auth) = cfg.auth {
                    builder = builder.proxy(p.basic_auth(&auth.username, &auth.password));
                }
                // noProxy: 需要为每个 host 创建 NoProxy 或使用 reqwest::NoProxy
            }
        }
    }
    builder.build().map_err(Into::into)
}
```

### 环境变量自动检测

```typescript
function resolveProxyConfig(proxy: boolean | string | ProxyConfig): ProxyConfig | null {
  if (proxy === false) return null
  if (proxy === true) {
    const url = process.env.HTTPS_PROXY || process.env.HTTP_PROXY || process.env.http_proxy || process.env.https_proxy
    if (!url) return null
    return { url, noProxy: process.env.NO_PROXY?.split(',') }
  }
  if (typeof proxy === 'string') return { url: proxy }
  return proxy
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 新增 ProxyConfig + HttpClientConfig.proxy |
| `catcher-http-ts/src/http/client.ts` | 创建代理 agent 并传给 axios |
| `catcher-ws-ts/src/ws/client.ts` | WS 连接传入代理 agent |
| `catcher-http/src/types/http.rs` | 新增 ProxyConfig |
| `catcher-http/src/transport/http_client.rs` | reqwest proxy 配置 |
| `catcher-ffi` | 暴露 proxy 配置 |

### 新增依赖

| 包 | 依赖 | 用途 |
|---|---|---|
| `catcher-http-ts` | `https-proxy-agent` | HTTP/HTTPS 代理 |
| `catcher-http-ts` | `socks-proxy-agent` | SOCKS5 代理 |

---

## G5: FormData / 文件上传

### 类型定义

```typescript
// catcher-core-ts/src/types.ts
export interface RequestConfig {
  // ...existing...
  /** 自动检测 FormData 并设置正确的 Content-Type */
  formData?: boolean  // 或者自动检测 body 类型
}
```

### `catcher-http-ts` — Node.js

**方案**: 在 `client.ts` 的 body 处理逻辑中自动检测 FormData 类型。

```typescript
// Node.js 端检测
// - Node.js >= 18: 全局 FormData 存在，body instanceof FormData
// - Node.js < 18 或使用 form-data npm 包: 需要检查构造函数名
function isFormDataBody(body: any): boolean {
  if (typeof FormData !== 'undefined' && body instanceof FormData) return true
  // form-data npm 包
  if (body?.constructor?.name === 'FormData') return true
  return false
}

// 在构建 axiosConfig 时
if (isFormDataBody(body)) {
  // 不设置 Content-Type，让 axios/form-data 自动添加 boundary
  delete axiosConfig.headers?.['Content-Type']
}
```

> **注意**：Node.js 18+ 才有全局 `FormData`。旧版本需使用 `form-data` npm 包，
> 其实例的 `constructor.name` 也是 `'FormData'`，但不通过 `instanceof` 检测。

提供便捷方法：

```typescript
// client.ts — 可选便捷方法
async upload(url: string, formData: FormData, config?: RequestConfig): Promise<any> {
  return this.post(url, formData, config)
}
```

### `catcher-web` — 浏览器

浏览器原生 `FormData` 对象可直接传给 `fetch`：

```typescript
// fetch options 中
if (body instanceof FormData) {
  // 不设置 Content-Type，让浏览器自动添加 boundary
  delete headers['Content-Type']
}
```

### Rust 层

```rust
// catcher-http/src/types/http.rs
pub enum Body {
    Bytes(Vec<u8>),
    Text(String),
    Multipart(Form),
}

pub struct Form {
    fields: Vec<(String, String)>,
    files: Vec<FilePart>,
}

pub struct FilePart {
    name: String,
    filename: String,
    content: Vec<u8>,
    mime: String,
}

// transport/http_client.rs — 构建 multipart
impl HttpTransport {
    pub async fn upload(&self, url: &str, form: Form) -> Result<HttpResponse, CatcherError> {
        let mut multipart = reqwest::multipart::Form::new();
        for (name, value) in form.fields {
            multipart = multipart.text(name, value);
        }
        for file in form.files {
            let part = reqwest::multipart::Part::bytes(file.content)
                .file_name(file.filename)
                .mime_str(&file.mime)?;
            multipart = multipart.part(file.name, part);
        }
        self.client.post(url).multipart(multipart).send().await?
    }
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 可选：formData 相关类型 |
| `catcher-http-ts/src/http/client.ts:265-268` | 自动检测 FormData body |
| `catcher-web/src/http/client.ts` | FormData body 处理 |
| `catcher-http/src/types/http.rs` | Body 枚举扩展 + Form/FilePart |
| `catcher-http/src/transport/http_client.rs` | multipart 发送 |

---

## G6: 重定向控制

### 类型定义

```typescript
export interface HttpClientConfig {
  // ...existing...
  redirect?: {
    follow?: boolean          // 默认 true
    maxRedirects?: number     // 默认 5
    beforeRedirect?: (info: RedirectInfo) => boolean
  }
}

export interface RedirectInfo {
  url: string
  status: number
  headers: Record<string, string>
}
```

### `catcher-http-ts` — axios

```typescript
// client.ts axiosConfig 构建
const axiosConfig: AxiosRequestConfig = {
  // ...existing...
  maxRedirects: config.redirect?.follow === false ? 0 : (config.redirect?.maxRedirects ?? 5),
  // axios 无 beforeRedirect 钩子，需要自定义 httpAgent 的 beforeRedirect（Node.js 原生支持）
}
```

`beforeRedirect` 需要使用 Node.js 原生 `http.Agent` 的 `beforeRedirect` 回调（Node.js >= 18.8），通过 axios 的 `httpAgent` / `httpsAgent` 传入。

### `catcher-web` — 浏览器

浏览器 `fetch` 的 `redirect` 选项：

```typescript
const fetchOptions: RequestInit = {
  redirect: config.redirect?.follow === false ? 'manual' : 'follow',
}
```

`fetch` 不提供 `maxRedirects` 和 `beforeRedirect`，需在拦截器中手动处理。

### Rust 层

```rust
// reqwest 原生支持
let mut builder = Client::builder();
builder = builder.redirect(if follow { Policy::limited(max_redirects) } else { Policy::none() });
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 新增 redirect 配置 |
| `catcher-http-ts/src/http/client.ts:240-259` | axiosConfig 传递 redirect |
| `catcher-web/src/http/client.ts` | fetch redirect 选项 |
| `catcher-http/src/types/http.rs` | HttpClientConfig 增加 redirect |
| `catcher-http/src/transport/http_client.rs` | reqwest redirect policy |

---

## G7: 自定义 Hostname 解析 (host_mapping)

### Rust 层 — 核心

**接入点**: `catcher-http/src/transport/http_client.rs` 中 reqwest Client 构建。

```rust
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;

// 自定义 DNS resolver
pub struct MappingResolver {
    host_mapping: HashMap<String, String>,  // hostname → IP
    fallback: Arc<dyn Resolve + Send + Sync>,
}

impl Resolve for MappingResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let mapping = self.host_mapping.clone();
        let fallback = self.fallback.clone();
        Box::pin(async move {
            if let Some(ip) = mapping.get(name.as_str()) {
                let addr: SocketAddr = format!("{}:0", ip).parse()?;
                return Ok(Box::new(std::iter::once(addr)) as Box<dyn Iterator<Item = SocketAddr> + Send>);
            }
            fallback.resolve(name).await
        })
    }
}

// 构建 client 时
let resolver = Arc::new(MappingResolver {
    host_mapping: config.dns.host_mapping.clone(),
    fallback: Arc::new(system_resolver),
});
let client = Client::builder()
    .dns_resolver(resolver)  // reqwest 支持 Arc<dyn Resolve>
    // 关键：TLS SNI 仍使用原始 hostname，不需要额外配置
    // reqwest 默认行为就是用 URL 中的 hostname 做 SNI
    .build()?;
```

### TS 层

```typescript
// catcher-core-ts/src/types.ts — DnsConfig
export interface DnsConfig {
  dnsCacheTtl?: number          // 已有
  nameservers?: string[]        // DNS 服务器
  hostMapping?: Record<string, string>  // hostname → IP
}

// catcher-http-ts — 通过 SharedAgent 注入自定义 DNS 解析
// cacheable-lookup 支持 custom lookup function
import CacheableLookup from 'cacheable-lookup'

const cacheable = new CacheableLookup()
// 覆盖 lookup 方法，优先查 hostMapping
const originalLookup = cacheable.lookup.bind(cacheable)
cacheable.lookup = (hostname, options, callback) => {
  const mapping = config.dns?.hostMapping
  if (mapping && mapping[hostname]) {
    // 返回映射 IP
    return callback(null, mapping[hostname], 4)
  }
  return originalLookup(hostname, options, callback)
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 新增 DnsConfig 类型 |
| `catcher-http-ts/src/agent/shared-agent.ts` | 注入 hostMapping 到 DNS lookup |
| `catcher-http/src/types/http.rs` | DnsConfig 增加 host_mapping |
| `catcher-http/src/transport/http_client.rs` | 自定义 Resolve trait 实现 |

---

## G8: HTTPS 配置增强

### Rust 层

```rust
// catcher-http/src/types/http.rs — 增强版 TlsConfig
pub struct TlsConfig {
    pub reject_unauthorized: bool,
    // CA
    pub ca_cert_path: Option<String>,
    pub ca_cert_pem: Option<String>,
    pub ca_cert_der: Option<Vec<u8>>,
    // 客户端证书 (mTLS)
    pub client_cert_path: Option<String>,
    pub client_cert_pem: Option<String>,
    pub client_key_path: Option<String>,       // ← 新增
    pub client_key_pem: Option<String>,        // ← 新增
    pub client_identity_pfx: Option<Vec<u8>>,  // ← 新增
    pub client_identity_password: Option<String>,
    // 高级
    pub tls_sni_override: Option<String>,
    pub min_tls_version: Option<TlsVersion>,
    pub max_tls_version: Option<TlsVersion>,
    pub pin_sha256: Option<Vec<String>>,
}

pub enum TlsVersion {
    Tls1_0, Tls1_1, Tls1_2, Tls1_3,
}
```

> **⚠️ `pin_sha256` 实现复杂度警告**：reqwest **不原生支持**证书公钥指纹 pinning。
> 实现需要自定义 `rustls` `ClientConfig`，通过 `ServerCertVerifier` trait
> 在握手后检查服务端证书公钥 SHA-256 摘要与配置的指纹列表匹配。
> 涉及 `reqwest::ClientBuilder::use_rustls_tls()` + `rustls::client::danger::ServerCertVerifier`。
> 考虑到复杂度较高，建议 **V1 不实现 `pin_sha256`**，在后续版本补充。

**接入点**: `http_client.rs` 中 reqwest Client 构建的 TLS 部分。

```rust
use reqwest::tls::{TlsVersion as ReqTlsVersion, Certificate, Identity};

let mut builder = Client::builder()
    .danger_accept_invalid_certs(!tls.reject_unauthorized);

// CA 证书
if let Some(ref pem) = tls.ca_cert_pem {
    let cert = Certificate::from_pem(pem.as_bytes())?;
    builder = builder.add_root_certificate(cert);
}
if let Some(ref path) = tls.ca_cert_path {
    let cert = Certificate::from_pem_file(path)?;
    builder = builder.add_root_certificate(cert);
}

// mTLS 客户端证书 + 私钥
// reqwest Identity::from_pem 接受 PEM 格式的 cert + key 合并 buffer
// 需要验证：cert PEM + key PEM 拼接后是否能被正确解析
// 备选方案：使用 Identity::from_pkcs12_der (PFX 格式，更可靠)
if let (Some(cert_pem), Some(key_pem)) = (&tls.client_cert_pem, &tls.client_key_pem) {
    let mut pem_buf = Vec::new();
    pem_buf.extend_from_slice(cert_pem.as_bytes());
    pem_buf.extend_from_slice(key_pem.as_bytes());
    let identity = Identity::from_pem(&pem_buf)?;
    builder = builder.identity(identity);
}
// 或者如果同时提供了 PFX:
if let Some(ref pfx) = tls.client_identity_pfx {
    let identity = Identity::from_pkcs12_der(pfx, tls.client_identity_password.as_deref().unwrap_or(""))?;
    builder = builder.identity(identity);
}

// TLS 版本控制
if let Some(ref min) = tls.min_tls_version {
    builder = builder.min_tls_version(match min {
        TlsVersion::Tls1_2 => ReqTlsVersion::TLS_1_2,
        TlsVersion::Tls1_3 => ReqTlsVersion::TLS_1_3,
        _ => ReqTlsVersion::TLS_1_2,
    });
}

// SNI 覆盖
if let Some(ref sni) = tls.tls_sni_override {
    builder = builder.tls_sni(sni.clone());
}
```

### TS 层

```typescript
export interface TlsConfig {
  rejectUnauthorized?: boolean
  caCertPath?: string
  caCertPem?: string
  clientCertPath?: string
  clientCertPem?: string
  clientKeyPath?: string
  clientKeyPem?: string
  tlsSniOverride?: string
  minTlsVersion?: '1.0' | '1.1' | '1.2' | '1.3'
  pinSha256?: string[]
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-http/src/types/http.rs` | TlsConfig 增加字段 |
| `catcher-http/src/transport/http_client.rs` | TLS 配置实现 |
| `catcher-core-ts/src/types.ts` | 新增 TlsConfig 类型 |
| `catcher-http-ts/src/agent/shared-agent.ts` | TLS 选项传递 |

---

## G9: Transport trait (自定义 Adapter)

### Rust 层

```rust
// catcher-http/src/transport/mod.rs — 新增
pub trait Transport: Send + Sync {
    fn execute(&self, request: HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, CatcherError>> + Send + '_>>;
}

// 现有 HttpTransport 实现此 trait
impl Transport for HttpTransport {
    fn execute(&self, request: HttpRequest) -> Pin<Box<dyn Future<Output = Result<HttpResponse, CatcherError>> + Send + '_>> {
        Box::pin(async move {
            self.execute_inner(request).await
        })
    }
}

// HttpClient 可注入自定义 Transport
pub struct HttpClient {
    transport: Arc<dyn Transport>,
    // ...existing fields...
}

impl HttpClient {
    pub fn with_transport(transport: Arc<dyn Transport>) -> Self { ... }
    pub fn new(config: HttpClientConfig) -> Self {
        // 默认使用 HttpTransport
        Self::with_transport(Arc::new(HttpTransport::new(config)))
    }
}
```

### TS 层

```typescript
// catcher-core-ts/src/types.ts
export interface TransportAdapter {
  execute(config: RequestConfig & { method: string; url: string; body?: any }): Promise<HttpResponse>
}

export interface HttpClientConfig {
  // ...existing...
  adapter?: TransportAdapter
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-http/src/transport/mod.rs` | 新增 Transport trait |
| `catcher-http/src/transport/http_client.rs` | HttpTransport impl Transport |
| `catcher-core-ts/src/types.ts` | 新增 TransportAdapter |
| `catcher-http-ts/src/http/client.ts` | 支持 adapter 注入 |

---

## G10: 流式响应

### 类型定义

```typescript
export interface RequestConfig {
  responseType?: 'json' | 'text' | 'bytes' | 'stream'
}
```

### `catcher-http-ts`

```typescript
// client.ts axiosConfig 中
const axiosConfig = {
  responseType: merged.responseType === 'stream' ? 'stream' :
                merged.responseType === 'bytes' ? 'arraybuffer' :
                merged.responseType,
}
```

> **⚠️ 流式模式与拦截器链的交互**：当 `responseType === 'stream'` 时，
> 响应拦截器仍会执行（`client.ts:280`），但拦截器拿到的是 stream 对象而非解析后的数据。
> 需要在响应拦截器调用前检查 `responseType`，流式模式下**跳过**响应拦截器链，
> 直接返回 `{ status, headers, data: response.data }`（data 为 Node.js Readable）。
> 这与其他 responseType 的行为不一致，需要在文档中明确说明。
```

### `catcher-web`

```typescript
// fetch 返回的 response.body 就是 ReadableStream
if (merged.responseType === 'stream') {
  return response.body  // Web ReadableStream<Uint8Array>
}
```

### Rust 层

```rust
pub enum ResponseBody {
    Json(serde_json::Value),
    Text(String),
    Bytes(Vec<u8>),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>),
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts:68` | responseType 增加 'stream' |
| `catcher-http-ts/src/http/client.ts:272-277` | stream 类型直接返回 |
| `catcher-web/src/http/client.ts` | 返回 response.body |
| `catcher-http/src/types/http.rs` | ResponseBody 增加 Stream variant |

---

## G11: 韧性运行时控制

### 事件系统

```typescript
// catcher-core-ts/src/types.ts — 新增事件类型
export type ClientEvent =
  | { type: 'retry'; attempt: number; error: Error; url: string }
  | { type: 'circuitBreakerChange'; from: 'closed' | 'open' | 'half-open'; to: 'closed' | 'open' | 'half-open' }
  | { type: 'networkQualityChange'; from: string; to: string }
  | { type: 'requestComplete'; method: string; url: string; status: number; durationMs: number }

export interface IHttpClient {
  // ...existing...
  on(event: ClientEvent['type'], listener: (event: ClientEvent) => void): () => void  // 返回 unsubscribe
  off(event: ClientEvent['type'], listener?: (event: ClientEvent) => void): void
}
```

### 接入点

**retry 事件**: `retry.ts:68-70` 的 `onFailedAttempt` 回调中 emit。
**CB 事件**: `client.ts` 中熔断器状态变更时 emit。
**requestComplete 事件**: `client.ts:272` 响应处理后 emit。

### 运行时配置热更新

```typescript
export interface IHttpClient {
  // ...existing...
  updateConfig(updates: Partial<Pick<HttpClientConfig, 'retry' | 'concurrency' | 'circuitBreaker' | 'timeout'>>): void
}
```

内部使用间接引用实现热更新：retry wrapper、circuit breaker 等组件通过闭包引用一个可变的 config 对象（而非直接捕获值）。`updateConfig` 修改这个共享对象，下次请求自动使用新配置。

```typescript
// 实现方式：所有韧性层组件引用同一个可变 config 对象
const mutableConfig = { retry, circuitBreaker, concurrency }

// updateConfig 修改 mutableConfig 并在必要时重建组件
function updateConfig(updates) {
  if (updates.retry) {
    mutableConfig.retry = updates.retry
    // 需要重新创建 retry wrapper，因为 pRetry 的 retries 参数在调用时传入
    // 但 pRetry 的 onFailedAttempt 等回调引用 mutableConfig，所以修改会生效
  }
  if (updates.concurrency && queue) {
    // p-queue 不支持运行时修改 concurrency，需要重建
    // 或者使用支持动态 concurrency 的队列实现
  }
}
```

> **⚠️ 实现约束**：
> - `p-retry` 的 `retries` 参数在每次 `pRetry()` 调用时传入，可以动态变更。
> - `p-queue` 的 `concurrency` 在构造时设定，**不支持运行时修改**。需要重建队列或使用支持动态并发的替代方案。
> - Cockatiel 的 `CircuitBreakerPolicy` 同样在构造时确定参数，修改需要重建。
> 建议简化：`updateConfig` 只支持修改 `retry` 配置，CB 和 concurrency 保持创建时不变。

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | 新增 ClientEvent 联合类型 |
| `catcher-http-ts/src/http/client.ts` | EventEmitter + updateConfig |
| `catcher-http-ts/src/http/retry.ts:68` | emit retry 事件 |
| `catcher-http/src/transport/http_client.rs` | RwLock<Config> + 事件通道 |

---

## G12: 认证辅助

### 类型定义

```typescript
export interface HttpClientConfig {
  // ...existing...
  auth?: { username: string; password: string }
  bearerToken?: string | (() => string | Promise<string>)
  xsrfCookieName?: string
  xsrfHeaderName?: string
}
```

### 实现

**接入点**: `client.ts:221-230` 请求拦截器中自动注入：

```typescript
// 作为内置请求拦截器
if (instanceConfig.auth) {
  const encoded = Buffer.from(`${instanceConfig.auth.username}:${instanceConfig.auth.password}`).toString('base64')
  defaultHeaders['Authorization'] = `Basic ${encoded}`
}

// Bearer token — 支持动态刷新
if (instanceConfig.bearerToken) {
  const resolveToken = typeof instanceConfig.bearerToken === 'function'
    ? instanceConfig.bearerToken
    : () => instanceConfig.bearerToken as string
  // 在请求拦截器中
  const token = await resolveToken()
  if (token) defaultHeaders['Authorization'] = `Bearer ${token}`
}

// XSRF — 从 cookie 读取并注入 header（浏览器端）
// ⚠️ 注意：如果 XSRF cookie 设置了 HttpOnly，则 JS 无法通过 document.cookie 读取。
// 此功能仅适用于非 HttpOnly 的 XSRF cookie。对于 HttpOnly 场景，
// 需要服务端通过响应 header 返回 XSRF token，或使用其他机制。
if (instanceConfig.xsrfCookieName && typeof document !== 'undefined') {
  const match = document.cookie.match(new RegExp(`(?:^|; )${instanceConfig.xsrfCookieName}=([^;]*)`))
  if (match) {
    defaultHeaders[instanceConfig.xsrfHeaderName ?? 'X-XSRF-TOKEN'] = decodeURIComponent(match[1])
  }
}
```

### 改动文件清单

| 文件 | 改动 |
|------|------|
| `catcher-core-ts/src/types.ts` | HttpClientConfig 增加 auth/bearerToken/xsrf |
| `catcher-http-ts/src/http/client.ts:130-140` | 内置认证拦截器 |
| `catcher-web/src/http/client.ts` | 浏览器端 XSRF 读取 |

---

## 实施顺序与依赖图

```
Phase 1 (P0 — 基础可用性):
┌──────────────────────────────────────┐
│  G2 (错误)  ←── 独立                  │
│  G3 (CORS)  ←── 独立，仅 TS 层        │
└──────────────────────────────────────┘
    ↓
Phase 2 (P1 — 独立功能):
┌──────────────────────────────────────┐
│  G4 (proxy)       ←── 依赖 https-proxy-agent 新依赖 │
│  G6 (重定向)      ←── 独立                           │
│  G5 (FormData)    ←── 独立                           │
│  G10 (流式响应)   ←── 独立                           │
└──────────────────────────────────────┘
    ↓
Phase 3 (P1 — 关联组):
┌──────────────────────────────────────┐
│  G7 (host_mapping) ─┐                │
│  G8 (HTTPS 增强)   ─┤ 互相关联       │
│  G9 (Transport)    ─┘ 一起做         │
└──────────────────────────────────────┘
    ↓
Phase 4 (P1 — 韧性):
┌──────────────────────────────────────┐
│  G11 (韧性运行时控制)                  │
└──────────────────────────────────────┘
    ↓
Phase 5 (P2):
┌──────────────────────────────────────┐
│  G12 (认证辅助) ←── 依赖 G3 (cookie)  │
└──────────────────────────────────────┘
```

### 每个 Phase 的预估改动量

| Phase | 新增类型 | TS 改动 | Rust 改动 | FFI 改动 | 新依赖 |
|-------|---------|---------|-----------|---------|--------|
| 1 | ~80 行 | ~100 行 | ~80 行 | ~20 行 | 无 |
| 2 | ~80 行 | ~200 行 | ~120 行 | ~40 行 | https-proxy-agent, socks-proxy-agent |
| 3 | ~150 行 | ~100 行 | ~300 行 | ~60 行 | 无 |
| 4 | ~50 行 | ~80 行 | ~150 行 | ~30 行 | 无 |
| 5 | ~30 行 | ~60 行 | — | — | 无 |
