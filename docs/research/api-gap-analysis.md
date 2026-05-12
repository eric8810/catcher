# API 能力差距分析：catcher vs axios vs dio

> 目的：从调用方视角审视 catcher 当前封装的 API 是否足以覆盖其提供的所有可配置特性，并与主流请求库 axios（JS/TS）和 dio（Dart）对比，识别需要补充的能力。
>
> **配套文档**：WebSocket 和 tus 上传协议的差距分析见 [ws-tus-gap-analysis.md](./ws-tus-gap-analysis.md)

---

## 一、当前 catcher API 一览

### Rust 核心层 (`catcher-rs`)

| API | 说明 |
|-----|------|
| `HttpTransport::new(config)` | 从 `HttpClientConfig` 创建客户端 |
| `HttpTransport::execute(request)` | 发送 `HttpRequest`，返回 `HttpResponse` |
| `WsTransport::connect(url, config)` | 建立 WebSocket，返回 `(WsHandle, Receiver<WsEvent>)` |
| `WsHandle::send_text/send_binary/close` | WS 发送与关闭 |
| `CircuitBreaker::new(config)` / `.call(op)` | 熔断器包装 |
| `retry_with_backoff(config, op, retry_if, on_retry)` | 通用重试 |
| `AdaptiveTimeout::new()` / `.record_rtt()` / `.compute()` | 自适应超时 |
| `NetworkQualityEvaluator` / `MetricsCollector` | 网络质量与指标 |

### TypeScript 封装层 (`catcher-ts`)

| API | 说明 |
|-----|------|
| `createHttpClient(config) → IHttpClient` | 创建 HTTP 客户端 (get/post/put/delete/patch) |
| `createResilientWS(options)` | 创建自恢复 WebSocket |
| `pack/unpack/isBinary/decodeWSMessage` | msgpack 编解码 |
| `createSharedAgent(opts?)` / `clearDnsCache()` | 共享 Agent + DNS 缓存 |
| `createPriorityQueue(opts?)` / `enqueueWithPriority()` | 优先级并发调度 |

---

## 二、逐项对比：catcher vs axios vs dio

### 2.1 请求构建与发送

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 便捷方法 (get/post/...) | ✅ `.get(url, config?)` | ✅ `.get(path, options?)` | ✅ `client.get<T>(url)` | — |
| 通用 request 方法 | ✅ `.request(config)` 支持任意 HTTP 方法 | ✅ `.request(path, options)` | ⚠️ `execute(HttpRequest)` 仅限 5 种方法 | **缺 HEAD/OPTIONS/TRACE 等方法** |
| URL 参数序列化 | ✅ `params` + `paramsSerializer` | ✅ `queryParameters` | ❌ 无 query params API | **调用方需自行拼 URL** |
| baseURL 拼接 | ✅ `baseURL` + 相对路径 | ✅ `baseUrl` + 相对路径 | ✅ `base_url` | — |
| 请求体自动序列化 | ✅ JSON / FormData / URLSearchParams 自动识别 | ✅ 根据 contentType 自动处理 | ❌ 仅接受 `Vec<u8>` / `Buffer` | **调用方需手动序列化** |

### 2.2 请求级配置覆盖 (Per-request Options)

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 请求级超时覆盖 | ✅ `config.timeout` | ✅ `Options.receiveTimeout` | ✅ `request.timeout_ms` | — |
| 请求级 headers | ✅ `config.headers` | ✅ `Options.headers` | ✅ `request.headers` | — |
| 请求级 responseType | ✅ json/text/stream/blob/arraybuffer | ✅ json/plain/bytes/stream | ❌ 始终返回原始字节 | **无法按请求选择解析方式** |
| 请求级重试策略覆盖 | ❌ 无原生支持 | ❌ 无原生支持 | ❌ 无 | — |
| 请求级 validateStatus | ✅ `(status) => boolean` | ✅ `ValidateStatus` 回调 | ❌ 5xx 即为错误 | **无法自定义成功状态码范围** |
| 请求级优先级 | ❌ 无 | ❌ 无 | ✅ `request.priority` | catcher 领先 |
| 请求级 extra/元数据 | ✅ `config.extra` | ✅ `Options.extra` | ❌ 无 | **无请求级自定义元数据透传** |

### 2.3 拦截器 / 中间件

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 请求拦截器 | ✅ `interceptors.request.use(fn)` | ✅ `onRequest(options, handler)` | ⚠️ 仅 TS 层 `interceptors.request[]` 静态数组 | **Rust 层无拦截器；TS 层无法动态增删** |
| 响应拦截器 | ✅ `interceptors.response.use(fn)` | ✅ `onResponse(response, handler)` | ⚠️ 同上 | 同上 |
| 错误拦截器 | ✅ 响应拦截器的 error 回调 | ✅ `onError(error, handler)` | ⚠️ TS 层 response error 回调 | **Rust 层无错误拦截** |
| 拦截器短路 (resolve/reject) | ✅ `handler.resolve()` / `handler.reject()` | ✅ `handler.resolve()` / `handler.reject()` | ❌ 无 | **无法在拦截器中提前返回/中止** |
| 拦截器动态增删 | ✅ `eject(id)` / `clear()` | ✅ `interceptors.add/remove` | ❌ 创建时固定 | **运行时不可变** |
| 条件执行拦截器 | ✅ `runWhen` | ❌ 手动判断 | ❌ 无 | — |
| 同步拦截器标记 | ✅ `{ synchronous: true }` | ❌ | ❌ 无 | — |
| QueuedInterceptor (串行化) | ❌ | ✅ `QueuedInterceptor` | ❌ 无 | — |
| 执行顺序控制 | ✅ 请求 LIFO / 响应 FIFO | ✅ 添加顺序 | ❌ 无 | — |

### 2.4 请求取消

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 取消机制 | ✅ `AbortController` / `signal` | ✅ `CancelToken` | ❌ 无 | **完全缺失** |
| 批量取消 | ✅ 共享 signal | ✅ 共享 CancelToken | ❌ 无 | — |
| 取消检测 | ✅ `axios.isCancel(err)` | ✅ `CancelToken.isCancel(err)` | ❌ 无 | — |

### 2.5 上传/下载进度

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 上传进度 | ✅ `onUploadProgress` | ✅ `onSendProgress` | ❌ 无 | **完全缺失** |
| 下载进度 | ✅ `onDownloadProgress` | ✅ `onReceiveProgress` | ❌ 无 | **完全缺失** |
| 流式响应 | ✅ `responseType: 'stream'` | ✅ `ResponseType.stream` | ❌ 无 | **无法流式读取响应** |

### 2.6 文件与表单

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| FormData 构建 | ✅ `toFormData(obj)` | ✅ `FormData.fromMap()` | ❌ 无 | **完全缺失** |
| 文件上传 | ✅ `MultipartFile` | ✅ `MultipartFile.fromFile()` | ❌ 无 | — |
| 文件下载 | ❌ 无原生 | ✅ `dio.download(url, savePath)` | ❌ 无 | — |
| x-www-form-urlencoded | ✅ 自动 | ✅ `Headers.formUrlEncodedContentType` | ❌ 无 | — |
| multipart/form-data | ✅ 自动 | ✅ 自动 | ❌ 无 | — |

### 2.7 响应处理

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 自动 JSON 解析 | ✅ 默认 | ✅ 默认 | ❌ 返回原始字节 | **调用方需手动解析** |
| 自定义响应转换 | ✅ `transformResponse` | ✅ `Transformer.transformResponse` | ❌ 无 | — |
| 自定义请求转换 | ✅ `transformRequest` | ✅ `Transformer.transformRequest` | ❌ 无 | — |
| 响应 schema 丰富度 | status + headers + data + config + request | status + headers + data + requestOptions + redirects + extra | status + headers + body + elapsed_ms | catcher 缺少原始请求回引 |
| 响应编码控制 | ✅ `responseEncoding` | ❌ | ❌ 无 | — |

### 2.8 错误处理

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 错误类型枚举 | ✅ `AxiosError` 含 code/isAxiosError/status | ✅ `DioExceptionType` 枚举 | ✅ `CatcherError` 枚举 | — |
| 错误中访问原始请求 | ✅ `error.config` | ✅ `error.requestOptions` | ❌ 无 | **错误丢失请求上下文** |
| 错误中访问响应 | ✅ `error.response` | ✅ `error.response` | ⚠️ 仅 `HttpError { status, body }` | body 是 String 而非原始字节 |
| 错误序列化 | ✅ `error.toJSON()` | ⚠️ toString | ❌ 无 | — |
| 错误脱敏 | ✅ `redact` 字段 | ❌ | ❌ 无 | — |
| 错误分类 | ⚠️ 用户自行判断 | ⚠️ 用户自行判断 | ✅ `ErrorCategory::Retryable/NonRetryable` | catcher 领先 |

### 2.9 配置与实例管理

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 实例创建 | ✅ `axios.create(config)` | ✅ `Dio(BaseOptions)` | ✅ `HttpTransport::new(config)` | — |
| 全局默认值 | ✅ `axios.defaults` | ✅ `dio.options` | ❌ 无全局默认 | **每个实例独立配置** |
| 运行时修改配置 | ✅ `instance.defaults.timeout = X` | ✅ `dio.options.receiveTimeout = X` | ❌ 创建后不可变 | **配置不可热更新** |
| 配置合并 | ✅ `mergeConfig(a, b)` | ✅ BaseOptions + Options 自动合并 | ❌ 无 | — |
| 克隆实例 | ❌ | ✅ `dio.clone()` | ❌ 无 | — |
| 实例销毁 | ❌ GC | ❌ GC | ✅ `catcher_http_client_destroy()` | catcher 领先 |

### 2.10 网络控制

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| 重定向控制 | ✅ `maxRedirects` / `beforeRedirect` / `followRedirects` | ✅ `followRedirects` / `maxRedirects` | ❌ 无 | **完全缺失** |
| 代理设置 | ❌ Node.js 层面 | ✅ `IOHttpClientAdapter` proxy | ❌ 无 | — |
 HTTPS 证书验证 | ✅ `rejectUnauthorized` | ✅ `validateCertificate` / `SecurityContext` | ⚠️ 仅 `reject_unauthorized` + `ca_cert_path` | **缺 SNI 覆盖 / 证书 pinning / client_key / DER 格式** |
| HTTP/2 | ✅ fetch adapter | ✅ `dio_http2_adapter` | ❌ 无 | — |
| 自定义 Adapter | ✅ `adapter` 选项 | ✅ `HttpClientAdapter` | ❌ 无 | **无法替换底层传输实现** |
| DNS 控制 | ❌ 系统 DNS | ❌ 系统 DNS | ⚠️ `DnsConfig` 仅缓存参数，有 nameservers 但无 host_mapping | **缺自定义 hostname→IP 映射** |
| 连接池控制 | ❌ Node.js Agent 层面 | ❌ 内部 | ✅ `ConnectionPoolConfig` | catcher 领先 |

### 2.11 认证

| 能力 | axios | dio | catcher | 差距 |
|------|-------|-----|---------|------|
| HTTP Basic Auth | ✅ `auth: { username, password }` | ❌ 手动 header | ❌ 无 | — |
| XSRF/CSRF Token | ✅ `xsrfCookieName` + `xsrfHeaderName` + `withXSRFToken` | ❌ 手动 | ❌ 无 | — |
| Bearer Token | ❌ 手动 header | ❌ 手动 header | ❌ 无 | — |

### 2.12 韧性特性（catcher 独有优势）

| 能力 | axios | dio | catcher |
|------|-------|-----|---------|
| 自动重试 + 退避 | ❌ (需第三方) | ❌ (需第三方) | ✅ 内建 |
| 熔断器 | ❌ | ❌ | ✅ 内建 |
| 优先级队列 + 并发控制 | ❌ | ❌ | ✅ 内建 |
| 自适应超时 | ❌ | ❌ | ✅ 内建 |
| 网络质量评估 | ❌ | ❌ | ✅ 内建 |
| WS 多端点竞速 | ❌ | ❌ | ✅ 内建 |
| WS 自适应心跳 | ❌ | ❌ | ✅ 内建 |
| 指标收集 | ❌ | ❌ | ✅ 内建 |

---

## 三、核心差距总结

### 🔴 P0 — 严重影响可用性，调用方迁移成本高

#### 1. 请求取消 (Cancellation)

**现状**：catcher 无任何取消机制。

**axios 方案**：`AbortController` + `signal`，标准 Web API。

**dio 方案**：`CancelToken`，可跨请求共享，`cancel(reason)` 批量取消。

**建议**：
```rust
// Rust 层
pub struct HttpRequest {
    // ...existing fields...
    pub cancel_token: Option<CancelToken>,
}

pub struct CancelToken { inner: tokio_util::sync::CancellationToken }

impl CancelToken {
    pub fn new() -> Self;
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    pub fn on_cancel(&self) -> impl Future<Output = ()>;
}
```
```typescript
// TS 层
client.get('/users', { signal: abortController.signal })
```

#### 2. 拦截器系统 (Interceptor System)

**现状**：TS 层仅在创建时传入静态函数数组，无法动态增删、无法短路、无法在拦截器中 resolve/reject。Rust 层完全无拦截器。

**axios 方案**：`interceptors.request.use(onFulfilled, onRejected, options?)`，返回 ID，可 `eject(id)` / `clear()`，支持 `synchronous` 和 `runWhen`。

**dio 方案**：`InterceptorsWrapper(onRequest, onResponse, onError)`，handler 可 `resolve()`/`reject()` 短路。还有 `QueuedInterceptor` 串行化。

**建议**：
```rust
// Rust 层 — 中间件 trait
pub trait RequestMiddleware: Send + Sync {
    async fn process_request(&self, request: HttpRequest, next: Next) -> Result<HttpResponse, CatcherError>;
}

pub struct Next<'a> { /* chain continuation */ }

// 支持动态增删
impl HttpTransport {
    pub fn add_middleware(&self, mw: Arc<dyn RequestMiddleware>) -> usize;
    pub fn remove_middleware(&self, id: usize);
}
```
```typescript
// TS 层
const id = client.interceptors.request.use((config) => { /* ... */ return config })
client.interceptors.request.eject(id)
client.interceptors.request.clear()
```

#### 3. 请求级配置覆盖 (Per-request Options)

**现状**：所有配置在创建时锁定。无法按请求覆盖 retry 策略、validateStatus、responseType 等。

**axios/dio 方案**：每次请求传入 `Options`，与实例级 `BaseOptions` 合并，后者覆盖前者。

**建议**：
```rust
pub struct RequestOptions {
    pub timeout_ms: Option<u64>,
    pub headers: Option<HashMap<String, String>>,
    pub retry: Option<RetryConfig>,           // 覆盖实例级重试
    pub validate_status: Option<fn(u16) -> bool>, // 自定义成功状态码
    pub response_type: Option<ResponseType>,  // json/text/bytes/stream
    pub priority: Option<Priority>,
    pub cancel_token: Option<CancelToken>,
    pub metadata: Option<HashMap<String, String>>, // 透传元数据
}

impl HttpTransport {
    pub async fn execute_with_options(&self, request: HttpRequest, options: RequestOptions) -> Result<HttpResponse, CatcherError>;
}
```

#### 4. 响应类型控制 (Response Type)

**现状**：始终返回 `Vec<u8>` 原始字节，调用方需手动反序列化。

**axios**：`responseType: 'json' | 'text' | 'stream' | 'blob' | 'arraybuffer' | 'formdata'`

**dio**：`ResponseType.json | .plain | .bytes | .stream`

**建议**：
```rust
pub enum ResponseType {
    Json,    // 自动 JSON 反序列化 → serde_json::Value
    Text,    // UTF-8 String
    Bytes,   // Vec<u8>（当前默认行为）
    Stream,  // 流式读取
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: ResponseBody,  // enum { Json(Value), Text(String), Bytes(Vec<u8>), Stream(...) }
    pub elapsed_ms: u64,
}
```

### 🟡 P1 — 影响开发体验和常见场景覆盖

#### 5. 上传/下载进度回调

**现状**：无任何进度通知机制。

**axios**：`onUploadProgress(progressEvent)` / `onDownloadProgress(progressEvent)`

**dio**：`onSendProgress(sent, total)` / `onReceiveProgress(received, total)`

**建议**：
```rust
pub struct ProgressEvent {
    pub loaded: u64,
    pub total: Option<u64>,
    pub rate_bytes_per_sec: Option<u64>,
}

pub struct RequestOptions {
    // ...
    pub on_upload_progress: Option<Arc<dyn Fn(ProgressEvent) + Send + Sync>>,
    pub on_download_progress: Option<Arc<dyn Fn(ProgressEvent) + Send + Sync>>,
}
```

#### 6. Query 参数序列化

**现状**：无 query params API，调用方必须自行拼 URL。

**axios**：`params: { key: value }` + `paramsSerializer`

**dio**：`queryParameters: { key: value }`

**建议**：
```rust
pub struct HttpRequest {
    // ...existing fields...
    pub query_params: Option<HashMap<String, String>>,
    pub params_serializer: Option<fn(&HashMap<String, String>) -> String>,
}
```

#### 7. FormData / 文件上传

**现状**：完全缺失。

**axios**：`toFormData(obj)` + multipart 自动处理

**dio**：`FormData.fromMap()` + `MultipartFile`

**建议**：
```rust
pub struct FormData {
    fields: Vec<(String, String)>,
    files: Vec<FileEntry>,
}

pub struct FileEntry {
    pub name: String,
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

impl HttpTransport {
    pub async fn upload(&self, url: &str, form: FormData) -> Result<HttpResponse, CatcherError>;
}
```

#### 8. 错误上下文丰富化

**现状**：`CatcherError` 枚举不携带原始请求信息，调用方 catch 到错误后无法得知是哪个 URL/方法/配置导致的。

**axios**：`AxiosError.config` / `.request` / `.response` / `.status` / `.toJSON()` / `.redact`

**dio**：`DioException.requestOptions` / `.response` / `.type` / `.stackTrace`

**建议**：
```rust
pub struct RequestError {
    pub kind: CatcherError,
    pub request: HttpRequest,              // 原始请求回引
    pub response: Option<HttpResponse>,    // 可选响应
    pub attempt: u32,                      // 第几次重试失败
    pub elapsed_ms: u64,
}
```

#### 9. 重定向控制

**现状**：无任何重定向控制。

**axios**：`maxRedirects` / `beforeRedirect` / `followRedirects`

**dio**：`followRedirects` / `maxRedirects`

**建议**：
```rust
pub struct HttpClientConfig {
    // ...existing fields...
    pub follow_redirects: bool,
    pub max_redirects: u32,
}
```

#### 10. 自定义 Hostname 解析 (Host Mapping)

**现状**：`DnsConfig` 仅有缓存参数（`cache_size / positive_ttl_secs / negative_ttl_secs`），plan 中增加了 `nameservers`（自定义 DNS 服务器），但仍需走 DNS 解析流程。无法直接指定 `api.example.com → 10.0.0.5`，这是企业内网、开发调试、灰度发布的常见需求。

**典型场景**：
- 开发环境将 `api.prod.com` 指向本地 mock 服务
- 灰度发布：将特定 hostname 路由到金丝雀 IP
- 内网直连：绕过 DNS，直接指定内网 IP，但 TLS SNI 仍保持原 hostname
- 多活容灾：同一 hostname 在不同地域解析到不同 IP

**axios**：无原生支持（需修改系统 hosts 文件或使用代理）

**dio**：无原生支持（需修改系统 hosts 文件或使用代理）

**建议**：
```rust
pub struct DnsConfig {
    // ...existing fields...
    pub nameservers: Vec<String>,                  // 自定义 DNS 服务器
    pub host_mapping: HashMap<String, String>,     // hostname → IP 直映射
}

// 在 build_dns_resolver 中实现：
// 1. 优先查 host_mapping，命中则直接返回映射 IP
// 2. 未命中则走 nameservers 或系统 DNS
// 3. TLS 握手时 SNI 仍使用原始 hostname（关键！）
```

```typescript
// TS 层
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  dns: {
    hostMapping: {
      'api.example.com': '10.0.0.5',      // 灰度 IP
      'ws.example.com': '192.168.1.100',  // 内网 WS
    },
  },
})
```

**实现要点**：
- reqwest 的 `dns_resolver()` 接受 `Arc<dyn Resolve>` trait，可自定义实现优先查 host_mapping
- 映射结果需保留原始 hostname 作为 SNI，否则 TLS 证书校验会失败
- host_mapping 应支持请求级覆盖（如灰度按用户路由）

#### 11. HTTPS 配置增强

**现状**：`TlsConfig` 仅有三个字段：
- `reject_unauthorized: bool` — 是否跳过证书验证
- `ca_cert_path: Option<String>` — CA 证书路径（仅 PEM 格式）
- `client_cert_path: Option<String>` — 客户端证书路径

**缺失项**：

| 缺失能力 | 说明 | 重要性 |
|---------|------|--------|
| `client_key_path` / `client_key_pem` | 客户端私钥，当前只有 cert 没有 key，mTLS 不可用 | 🔴 严重 |
| CA 证书内联 (`ca_cert_pem`) | 当前仅支持文件路径，不支持内存中的 PEM 字符串 | 🟡 中 |
| 证书格式多样性 | 仅 PEM，缺 DER / PKCS12 / PFX | 🟡 中 |
| SNI 覆盖 (`tls_dns_name`) | 自定义 TLS SNI hostname，配合 host_mapping 使用 | 🟡 中 |
| 证书 Public Key Pinning | 固定证书公钥指纹，防中间人 | 🟢 低 |
| TLS 协议版本控制 | 指定最低/最高 TLS 版本 | 🟢 低 |
| 证书吊销检查 (CRL/OCSP) | 运行时验证证书未被吊销 | 🟢 低 |

**dio 方案**：
- `validateCertificate` 回调 — 自定义证书验证逻辑
- `SecurityContext` — 系统级证书管理
- 指纹 pinning — `sha256.convert(cert.der)` 对比

**建议**：
```rust
pub struct TlsConfig {
    pub reject_unauthorized: bool,

    // CA 证书
    pub ca_cert_path: Option<String>,          // 文件路径（现有）
    pub ca_cert_pem: Option<String>,           // PEM 内联（plan 已规划）
    pub ca_cert_der: Option<Vec<u8>>,          // DER 格式

    // 客户端证书（mTLS）
    pub client_cert_path: Option<String>,      // 文件路径（现有）
    pub client_cert_pem: Option<String>,       // PEM 内联
    pub client_key_path: Option<String>,       // 私钥文件路径 ← 新增
    pub client_key_pem: Option<String>,        // 私钥 PEM 内联 ← 新增
    pub client_identity_pfx: Option<Vec<u8>>,  // PKCS12/PFX 格式 ← 新增
    pub client_identity_password: Option<String>, // PFX 密码

    // SNI / 高级
    pub tls_dns_name_override: Option<String>, // 自定义 SNI hostname ← 新增
    pub min_tls_version: Option<TlsVersion>,   // TLS 协议版本控制 ← 新增
    pub max_tls_version: Option<TlsVersion>,
    pub pin_sha256: Option<Vec<String>>,       // 证书公钥指纹 pinning ← 新增
}

pub enum TlsVersion {
    Tls1_0,
    Tls1_1,
    Tls1_2,
    Tls1_3,
}
```

```typescript
// TS 层
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  tls: {
    rejectUnauthorized: true,
    caCertPem: '-----BEGIN CERTIFICATE-----\n...',  // 内联 CA
    clientCertPem: '-----BEGIN CERTIFICATE-----\n...', // mTLS
    clientKeyPem: '-----BEGIN PRIVATE KEY-----\n...',  // mTLS 私钥
    tlsDnsNameOverride: 'api.example.com',  // 配合 hostMapping
    minTlsVersion: '1.2',
    pinSha256: ['ee5ce1dfa7a53657...'],      // 证书 pinning
  },
})
```

#### 12. 自定义 Adapter / 传输替换

**现状**：传输层硬编码为 reqwest，无法替换。

**axios**：`adapter` 函数可替换为 fetch/xhr/http 或自定义实现

**dio**：`HttpClientAdapter` 接口可替换为 IO/Browser/HTTP2 实现

**建议**：
```rust
pub trait Transport: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError>;
}

// HttpTransport 实现此 trait
// 调用方可实现 mock/缓存/代理等 adapter
```

### 🟢 P2 — 锦上添花，提升开发者友好度

#### 13. 认证辅助

HTTP Basic Auth 便捷字段、Bearer Token 自动刷新 hook、XSRF/CSRF 自动处理。

#### 14. 配置合并策略

类似 `axios.mergeConfig(a, b)`，定义实例配置与请求配置的合并优先级。

#### 15. 实例克隆

`dio.clone()` 式的实例复制，方便基于同一基座创建不同配置的客户端。

#### 16. Headers API 丰富化

当前 `HashMap<String, String>` 缺少大小写不敏感、便捷 get/set/has/delete/normalize 操作（axios 的 `AxiosHeaders` 类）。

#### 17. 请求 ID / 全链路追踪

为每个请求自动生成唯一 ID，穿透拦截器 → 重试 → 熔断 → 指标，方便日志关联。

#### 18. 内容安全限制

类似 axios 的 `maxContentLength` / `maxBodyLength` 防解压炸弹攻击。

#### 19. 响应转换 hook

`transformRequest` / `transformResponse` 允许调用方在拦截器之前/之后修改数据格式。

---

## 四、catcher 韧性特性的 API 可控性审视

catcher 的核心价值在于韧性特性，但当前 API 对这些特性的**运行时可控性**不足：

| 特性 | 创建时配置 | 运行时调整 | 运行时查询 | 事件通知 |
|------|-----------|-----------|-----------|---------|
| 重试 | ✅ RetryConfig | ❌ 不可调 | ❌ 无查询 | ⚠️ 仅 on_retry 回调 |
| 熔断器 | ✅ CircuitBreakerConfig | ❌ 不可调 | ❌ 无状态查询 | ❌ 无事件 |
| 自适应超时 | ✅ 构造参数 | ❌ 不可调 | ⚠️ compute() 可查 | ❌ 无事件 |
| 网络质量 | ✅ 窗口大小 | ❌ 不可调 | ✅ evaluate() 可查 | ❌ 无事件 |
| 优先级队列 | ✅ 并发数 | ❌ 不可调 | ❌ 无查询 | ❌ 无事件 |
| 并发控制 | ✅ max_concurrency | ❌ 不可调 | ❌ 无队列深度 | ❌ 无事件 |

**建议补充**：

1. **运行时配置热更新**：`client.update_config(|c| c.retry.max_attempts = 5)`
2. **状态查询 API**：`client.circuit_breaker_state()` / `client.queue_depth()` / `client.network_quality()`
3. **事件订阅**：
   ```rust
   client.on_event(|event| match event {
       Event::RetryAttempt { attempt, error } => { /* ... */ },
       Event::CircuitBreakerStateChanged { from, to } => { /* ... */ },
       Event::NetworkQualityChanged { from, to } => { /* ... */ },
       Event::RequestCompleted { duration, status } => { /* ... */ },
   });
   ```

---

## 五、FFI 层 API 完整性审视

当前 FFI 层（`09-ffi.md`）的函数签名过于简陋：

| 问题 | 现状 | 建议 |
|------|------|------|
| 请求配置仅 URL+body | `catcher_http_get(handle, url, callback)` | 增加 `RequestOptions` JSON 参数 |
| 无取消 | 无 | 增加 `catcher_http_cancel(handle, request_id)` |
| 无进度 | 无 | 增加 `on_progress` 回调 |
| 错误信息单薄 | `FfiResult { error_code, error_message }` | 增加 `request_context` 字段 |
| 无拦截器注册 | 无 | 增加 `catcher_http_add_request_interceptor(handle, callback)` |
| 无状态查询 | 无 | 增加 `catcher_http_get_metrics(handle)` / `catcher_cb_state(handle)` |

---

## 六、优先级路线图

| 优先级 | 补充能力 | 影响范围 | 预估工作量 |
|--------|---------|---------|-----------|
| P0-1 | 请求取消 (CancelToken/AbortSignal) | Rust + TS + FFI | 中 |
| P0-2 | 拦截器系统 (动态增删 + 短路 + 错误拦截) | Rust + TS + FFI | 大 |
| P0-3 | Per-request Options (覆盖 retry/responseType/validateStatus) | Rust + TS + FFI | 中 |
| P0-4 | ResponseType 枚举 (JSON/Text/Bytes/Stream) | Rust + TS + FFI | 中 |
| P1-1 | 上传/下载进度回调 | Rust + TS + FFI | 中 |
| P1-2 | Query 参数序列化 | Rust + TS | 小 |
| P1-3 | FormData / 文件上传 | Rust + TS + FFI | 大 |
| P1-4 | 错误上下文丰富化 (原始请求回引) | Rust + TS | 小 |
| P1-5 | 重定向控制 | Rust + TS | 小 |
| P1-6 | 自定义 Hostname 解析 (host_mapping) | Rust + TS + FFI | 中 |
| P1-7 | HTTPS 配置增强 (client_key/SNI/pinning/PFX) | Rust + TS + FFI | 中 |
| P1-8 | Transport trait (自定义 Adapter) | Rust | 中 |
| P2-1 | 认证辅助 | TS | 小 |
| P2-2 | 配置合并策略 | Rust + TS | 小 |
| P2-3 | 实例克隆 | Rust + TS | 小 |
| P2-4 | Headers API 丰富化 | Rust + TS | 小 |
| P2-5 | 请求 ID / 全链路追踪 | Rust + TS + FFI | 中 |
| P2-6 | 内容安全限制 | Rust | 小 |
| P2-7 | 响应转换 hook | Rust + TS | 小 |
| 韧性 | 运行时配置热更新 | Rust + TS | 中 |
| 韧性 | 状态查询 API | Rust + TS + FFI | 小 |
| 韧性 | 事件订阅机制 | Rust + TS + FFI | 中 |

---

## 七、结论

catcher 在**韧性特性**（重试/熔断/自适应超时/网络质量/优先级调度）上显著超越 axios 和 dio，这是其核心护城河。

但在**请求库基础能力**上存在明显短板，主要集中在三个维度：

1. **生命周期可控性**：拦截器动态管理、请求取消、配置热更新 — 调用方对请求生命周期的控制力不足
2. **数据格式灵活性**：响应类型选择、自动序列化、FormData、query 参数 — 调用方需大量手动处理
3. **可观测性**：错误上下文、状态查询、事件通知 — 调用方难以感知内部状态变化
4. **网络层灵活性**：自定义 hostname 解析、HTTPS/mTLS 配置、SNI 覆盖 — 企业场景刚需但 axios/dio 均不完善

建议按 P0 → P1 → 韧性增强 → P2 的顺序逐步补齐，确保 catcher 既是一个强大的韧性库，也是一个完整可用的请求库。

特别值得注意的是，**自定义 hostname 解析**和 **HTTPS 配置增强**这两项在 axios/dio 中同样缺失或仅部分支持，catcher 在此领域有机会实现差异化优势 — 尤其是配合 host_mapping + SNI 覆盖的组合，可以优雅解决企业内网直连、灰度发布、多活容灾等生产场景中修改系统 hosts 文件的痛点。
