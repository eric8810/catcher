# 16 — napi TS Wrapper 架构设计

> 状态：已通过 review 修正
> Review 记录：`docs/plan/11-napi-typed-config-design-review.md`
> 范围：`@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws`
> 实施计划：`docs/plan/11-napi-typed-config-design.md`

---

## 1. 现状分析

### 1.1 当前 API 调用方式

所有 napi 包的配置入口均接受 **JSON 字符串**，在 Rust 侧通过 `serde_json::from_str` 反序列化：

| 包 | 构造函数 | 配置参数类型 |
|----|---------|------------|
| `catcher-napi-http` | `new HttpClient(configJson: string)` | JSON string |
| `catcher-napi-http` | `sseStream(configJson: string, onEvent)` | JSON string |
| `catcher-napi-http` | `sseClient(configJson: string, onEvent)` | JSON string |
| `catcher-napi-ws` | `new WsClient(configJson: string, onEvent?)` | JSON string |

**Rust 侧关键代码**（`client.rs:94-96`）：
```rust
pub fn new(config_json: String) -> napi::Result<Self> {
    let config: HttpClientConfig = serde_json::from_str(&config_json)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    // ...
}
```

### 1.2 现有"弥补"手段：JS wrapper + 手写 `.d.ts`

两个包各有一层 JS wrapper（`client.js`）和一份手写的 `client.d.ts`：

**`client.js` 做的事**：
- 加载平台对应的 `.node` 二进制
- 包装原始 class，允许 `string | object` 参数（`typeof config === 'string' ? config : JSON.stringify(config)`）

**`client.d.ts` 手写了类型声明**，例如 napi-http 的 `client.d.ts:2-38`：
```typescript
interface HttpClientConfig {
  base_url?: string
  connect_timeout_ms?: number
  pool?: { keep_alive?: boolean; max_idle_per_host?: number; ... }
  retry?: { max_attempts?: number; backoff?: 'Fixed' | 'Exponential' | 'DecorrelatedJitter'; ... }
  circuit_breaker?: { failure_threshold?: number; ... }
}
export class HttpClient {
  constructor(config: string | HttpClientConfig)
}
```

### 1.3 同时存在的 napi 自动生成 `index.d.ts`

`napi build` 自动生成的 `index.d.ts` 使用原始命名（如 `JsHttpClient`、`configJson: string`），**与手写的 `client.d.ts` 存在不一致**：

| 差异点 | `index.d.ts`（自动生成） | `client.d.ts`（手写） |
|--------|------------------------|---------------------|
| 类名 | `JsHttpClient` | `HttpClient` |
| 配置参数 | `configJson: string` | `config: string \| HttpClientConfig` |
| 属性名 | `elapsedMs` | `elapsed_ms`（不一致） |
| SSE 配置类型 | 无声明 | 无声明 |

---

## 2. 问题清单

### P1: 缺少编译期类型安全

用户传入 JSON string 时，拼写错误不会被 IDE 发现，运行时静默使用默认值。

### P2: 事件回调全为 JSON string

所有事件（WS 事件、SSE 事件、Stream chunk）均以 JSON string 回调，用户每次手动 `JSON.parse`，且 `client.d.ts` 中的 `WsEvent` 缺少 `Reconnecting`、`HeartbeatRtt` 两个变体。

### P3: `.d.ts` 与 `.js` 分离 — 类型漂移的根源

wrapper 用纯 JS 编写，类型声明用手写 `.d.ts`，**两者没有编译器强制关联**。已出现 `elapsedMs` vs `elapsed_ms` 不一致。这是所有问题的根本原因。

| 维度 | `.d.ts` 声明文件 | `.ts` 源码 |
|------|-----------------|-----------|
| 类型与实现 | 分离 — 两处维护 | 同源 — 类型就是实现的类型 |
| 漂移风险 | 高（已发生） | 无 — 编译器保证一致 |
| wrapper 的类型检查 | 无 | 有 — wrapper 本身被 tsc 检查 |
| 发布产物 | 手写 `.d.ts` + `.js` | 自动生成 `.d.ts` + `.js` |

### P4: SSE 函数缺少类型声明

`index.d.ts` 中 SSE 函数签名只声明了 `configJson: string`，没有 `SseClientConfig` 类型定义。

---

## 3. 设计目标

1. **单一真相源**：用 `.ts` 编写 wrapper，`tsup` 自动生成 `.d.ts`，消除手写声明漂移
2. **类型安全**：配置对象有完整的 TypeScript 类型提示和编译期校验
3. **事件类型化**：回调事件从 `string` 升级为强类型联合，wrapper 自动 `JSON.parse`
4. **向后兼容**：保留 `string` 参数支持，不破坏现有用户代码
5. **camelCase 友好**（Phase 2）：Rust serde alias 同时支持 snake_case 和 camelCase

---

## 4. 方案设计

### 4.1 核心决策

将 `client.js` 改为 `ts/client.ts`，类型直接写在函数签名上。`tsup` 构建自动生成 `.d.ts`，**不可能漂移**。删除手写的 `client.js` 和 `client.d.ts`。

### 4.2 目录结构

```
packages/catcher-napi-http/
├── src/                        # Rust 源码（不变）
│   ├── lib.rs
│   ├── client.rs
│   ├── sse.rs
│   └── helpers.rs
├── ts/                         # TypeScript wrapper（新增）
│   ├── types.ts                # 配置 + 事件类型定义
│   ├── native.ts               # 原生模块加载
│   ├── client.ts               # HttpClient wrapper
│   └── sse.ts                  # SseStream / SseClient wrapper
├── dist/                       # tsup 构建输出（gitignore）
│   ├── client.js               # ← 发布用
│   ├── client.d.ts             # ← 自动生成
│   ├── types.js
│   ├── types.d.ts
│   ├── sse.js
│   └── sse.d.ts
├── index.js                    # napi 自动生成（不变）
├── index.d.ts                  # napi 自动生成（不变）
├── package.json
└── tsconfig.json

packages/catcher-napi-ws/
├── src/                        # Rust 源码（不变）
│   └── lib.rs
├── ts/
│   ├── types.ts
│   ├── native.ts
│   └── client.ts               # WsClient wrapper
├── dist/                       # tsup 构建输出
│   ├── client.js
│   ├── client.d.ts
│   ├── types.js
│   └── types.d.ts
├── index.js
├── index.d.ts
├── package.json
└── tsconfig.json
```

### 4.3 TypeScript 源码

#### 4.3.1 `ts/native.ts` — 原生模块加载

从现有 `client.js` 提取平台加载逻辑。注意：tsup 输出到 `dist/`，所以 `__dirname` 是 `dist/`，需要 `..` 回到包根目录。

```typescript
import os from 'os'
import path from 'path'

function tryRequire(...paths: string[]): any {
  for (const p of paths) {
    try { return require(p) } catch {}
  }
  return null
}

/**
 * 加载 napi 原生模块
 *
 * tsup 输出到 dist/，__dirname = dist/，
 * 所以所有路径都需要 path.join(__dirname, '..') 回到包根。
 */
export function loadNativeAddon(pkgName: string): any {
  const platform = os.platform()
  const arch = os.arch()
  // __dirname = dist/，root = 包根目录
  const root = path.join(__dirname, '..')

  // 1. index.js — napi build 生成的入口（在包根）
  const napiJs = tryRequire(path.join(root, 'index.js'))
  if (napiJs) return napiJs

  // 2. 预编译二进制 → 3. 根目录 .node → 4. cargo build 产物
  const libName = pkgName.replace(/-/g, '_')
  const addon =
    tryRequire(path.join(root, 'npm', `${platform}-${arch}`, `${pkgName}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.node`)) ??
    tryRequire(path.join(root, `${pkgName}.${platform}-${arch}.node`)) ??
    tryRequire(path.join(root, 'target', 'release', `lib${libName}.so`)) ??
    tryRequire(path.join(root, 'target', 'release', `lib${libName}.dylib`)) ??
    tryRequire(path.join(root, 'target', 'release', `${libName}.dll`))

  if (!addon) {
    throw new Error(
      `@eric8810/${pkgName}: native addon not found.\n` +
      `Run \`npm run build\` in packages/${pkgName} (requires Rust).`
    )
  }

  return addon
}
```

#### 4.3.2 `ts/types.ts` — 类型定义（napi-http）

类型与 Rust struct **精确对齐**，字段名保持 snake_case（Phase 2 加入 camelCase alias）。

```typescript
// ── napi-http 配置 + 事件类型 ──
// 所有字段与 Rust HttpClientConfig 一一对应
// 可选字段标注默认值，参考 docs/user-manual/api/napi.md

/** 连接池配置 — 对应 Rust PoolConfig */
export interface PoolConfig {
  /** 是否启用 TCP keepalive。默认: true */
  keep_alive?: boolean
  /** 每 host 最大空闲连接数。默认: 10 */
  max_idle_per_host?: number
  /** 空闲连接超时（秒）。默认: 30 */
  idle_timeout_secs?: number
  /** keepalive 探测间隔（秒）。默认: 20 */
  keep_alive_interval_secs?: number
}

/** TLS 配置 — 对应 Rust TlsConfig（完整 14 字段） */
export interface TlsConfig {
  /** 是否验证服务端证书。默认: true */
  reject_unauthorized?: boolean
  /** CA 证书 PEM 内容 */
  ca_cert_pem?: string
  /** CA 证书文件路径 */
  ca_cert_path?: string
  /** 客户端证书 PEM 内容 */
  client_cert_pem?: string
  /** 客户端证书文件路径 */
  client_cert_path?: string
  /** 客户端私钥 PEM 内容 */
  client_key_pem?: string
  /** 客户端私钥文件路径 */
  client_key_path?: string
  /** PFX/PKCS12 客户端身份（二进制） */
  client_identity_pfx?: Uint8Array
  /** PFX 身份密码 */
  client_identity_password?: string
  /** TLS SNI 覆写 */
  tls_sni_override?: string
  /** 最低 TLS 版本 */
  min_tls_version?: 'Tls1_0' | 'Tls1_1' | 'Tls1_2' | 'Tls1_3'
  /** 最高 TLS 版本 */
  max_tls_version?: 'Tls1_0' | 'Tls1_1' | 'Tls1_2' | 'Tls1_3'
  /** SHA-256 公钥指纹 pinning（deferred） */
  pin_sha256?: string[]
}

/** DNS 配置 — 对应 Rust DnsConfig */
export interface DnsConfig {
  /** DNS 缓存 TTL（秒）。默认: 300 */
  cache_ttl_secs?: number
  /** 自定义 DNS 服务器 */
  nameservers?: string[]
  /** Hostname → IP 映射 */
  host_mapping?: Record<string, string>
}

/** 退避策略 */
export type BackoffStrategy = 'Fixed' | 'Exponential' | 'DecorrelatedJitter'

/** 重试配置 — 对应 Rust RetryConfig */
export interface RetryConfig {
  /** 最大重试次数。默认: 3 */
  max_attempts?: number
  /**
   * 退避策略。
   * serde 字段级默认: 'Fixed'（BackoffKind::default）
   * RetryConfig::default(): 'Exponential'
   * 即：当 retry 对象存在但省略 backoff 时为 Fixed；retry 整体省略时取决于 Rust 调用方
   */
  backoff?: BackoffStrategy
  /** 最小退避延迟（ms）。默认: 100 */
  min_backoff_ms?: number
  /** 最大退避延迟（ms）。默认: 10000 */
  max_backoff_ms?: number
  /** 是否加入随机抖动。默认: true */
  jitter?: boolean
}

/** 熔断器配置 — 对应 Rust CircuitBreakerConfig */
export interface CircuitBreakerConfig {
  /** 连续失败多少次进入 OPEN。默认: 5 */
  failure_threshold?: number
  /** HALF_OPEN 连续成功多少次恢复 CLOSED。默认: 2 */
  success_threshold?: number
  /** OPEN → HALF_OPEN 等待时间（ms）。默认: 30000 */
  reset_timeout_ms?: number
  /** HALF_OPEN 状态最大放行请求数。默认: 5 */
  half_open_max_requests?: number
}

/** 代理认证 */
export interface ProxyAuth {
  username: string
  password: string
}

/**
 * 代理配置 — 对应 Rust ProxyConfig（判别联合，按 `mode` 区分）。
 *
 * - Manual（默认，省略 `mode` 或 `mode: 'manual'`）：必须提供 `url`。
 * - System（`mode: 'system'`）：自动从 OS 读取系统代理（需构建时启用 `system-proxy`
 *   feature），`url` 可省略。注意：System 模式仅在调用 `networkChanged()` 后才会
 *   探测并应用系统代理，首次构建时走直连。
 */
export type ProxyConfig = ManualProxyConfig | SystemProxyConfig

/** 手动代理（默认）。`url` 必填。 */
export interface ManualProxyConfig {
  mode?: 'manual'
  /** 代理 URL。Catcher 会把 socks5:// 按 socks5h:// 处理，避免代理场景提前本地解析域名。 */
  url: string
  auth?: ProxyAuth
  /** 不走代理的 hostname 列表 */
  no_proxy?: string[]
}

/** 系统代理：自动从 OS 检测（需构建时启用 `system-proxy` feature）。 */
export interface SystemProxyConfig {
  mode: 'system'
  /** 忽略；系统代理由 `detect_system_proxy()` 在 `networkChanged()` 时解析。 */
  url?: string
  auth?: ProxyAuth
  /** 不走代理的 hostname 列表 */
  no_proxy?: string[]
}

/** 重定向配置 — 对应 Rust RedirectConfig */
export interface RedirectConfig {
  /** 是否跟随重定向。默认: true */
  follow?: boolean
  /** 最大重定向次数。默认: 5 */
  max_redirects?: number
}

/**
 * HTTP 客户端配置 — 对应 Rust HttpClientConfig
 *
 * 所有可选字段有合理默认值，只需传入需要覆盖的字段。
 * 字段名使用 snake_case（与 Rust 一致）。
 */
export interface HttpClientConfig {
  /** 基础 URL，会与请求路径拼接 */
  base_url?: string
  /** 连接超时（ms）。默认: 10000 */
  connect_timeout_ms?: number
  /** 响应超时（ms）。默认: 30000 */
  response_timeout_ms?: number
  /** 连接池配置 */
  pool?: PoolConfig
  /** TLS 配置 */
  tls?: TlsConfig
  /** DNS 配置 */
  dns?: DnsConfig
  /** 重试配置 */
  retry?: RetryConfig
  /** 熔断器配置 */
  circuit_breaker?: CircuitBreakerConfig
  /** 最大并发请求数。默认: 50 */
  max_concurrency?: number
  /** 默认请求头（每次请求自动携带） */
  default_headers?: Record<string, string>
  /** HTTP DNS 场景的 Hostname 覆写 */
  hostname_override?: string
  /** 代理配置 */
  proxy?: ProxyConfig
  /** 重定向配置 */
  redirect?: RedirectConfig
  /** Basic 认证 */
  auth?: ProxyAuth
  /** Bearer token */
  bearer_token?: string
}

/** SSE 客户端配置 — 对应 Rust SseClientConfig */
export interface SseClientConfig {
  /** SSE 端点 URL */
  url: string
  /** HTTP 方法。默认: 'GET' */
  method?: 'GET' | 'POST'
  /** 请求头（如 Authorization） */
  headers?: Record<string, string>
  /** 请求体（POST 时使用） */
  body?: string
  /** 请求超时（ms）。默认: 30000 */
  timeout_ms?: number
  /** 自动重连配置 — 对应 Rust SseReconnectConfig */
  reconnect?: {
    /** 最大重试次数。默认: 10 */
    max_retries?: number
    /** 初始退避延迟（ms）。默认: 1000 */
    initial_delay_ms?: number
    /** 最大退避延迟（ms）。默认: 30000 */
    max_delay_ms?: number
    /** 退避乘数。默认: 2.0 */
    backoff_multiplier?: number
  }
  /** 熔断器配置 — 复用 CircuitBreakerConfig */
  circuit_breaker?: CircuitBreakerConfig
}

/** HTTP 响应 — 对应 Rust JsHttpResponse */
export interface HttpResponse {
  status: number
  headers: Record<string, string>
  body: Buffer
  elapsed_ms: number
}

/** 运行时指标 — 对应 Rust JsMetrics */
export interface Metrics {
  http_requests: number
  http_success_rate: number
  http_avg_latency_us: number
  http_retries: number
  ws_connect_success_rate: number
  ws_disconnects: number
  ws_messages_sent: number
  ws_messages_received: number
  cb_open_count: number
  queue_timeouts: number
}

/** 每请求选项 — 对应 Rust RequestOptions */
export interface RequestOptions {
  headers?: Record<string, string>
  timeout_ms?: number
  content_type?: string
}

/** 流式下载事件 — executeStream 回调参数 */
export type StreamEvent =
  | { type: 'Headers'; status: number; headers: Record<string, string> }
  | { type: 'Chunk'; data: string }   // base64 编码
  | { type: 'Done' }
  | { type: 'Error'; message: string }

/** SSE 事件 */
export type SseEvent =
  | { type: 'Line'; data: string }
  | { type: 'Error'; message: string }
  | { type: 'End' }
```

#### 4.3.3 `ts/types.ts` — 类型定义（napi-ws）

```typescript
// ── napi-ws 配置 + 事件类型 ──

/** 重连配置 — 对应 Rust ReconnectConfig */
export interface ReconnectConfig {
  /** 初始退避延迟（ms）。默认: 500 */
  initial_delay_ms?: number
  /** 最大退避延迟（ms）。默认: 30000 */
  max_delay_ms?: number
  /** 退避乘数。默认: 2.0 */
  backoff_multiplier?: number
  /** 最大重试次数。默认: 20 */
  max_attempts?: number
}

/** 心跳配置 — 对应 Rust HeartbeatConfig */
export interface HeartbeatConfig {
  /** 心跳间隔（ms）。默认: 30000 */
  interval_ms?: number
  /** 是否根据 RTT 自适应调整。默认: true */
  adaptive?: boolean
  /** pong 超时（ms）。默认: 10000 */
  pong_timeout_ms?: number
  /** 连续丢失多少个 pong 判定断线。默认: 3 */
  max_missed_pongs?: number
}

/**
 * WebSocket 客户端配置 — 对应 Rust WsClientConfig
 */
export interface WsClientConfig {
  /** 端点 URL 列表（多端点竞速） */
  urls: string[]
  /** 子协议 */
  protocols?: string[]
  /** 自定义 headers */
  headers?: Record<string, string>
  /** 启用 perMessageDeflate 压缩。默认: false */
  per_message_deflate?: boolean
  /** 压缩阈值（字节）。默认: 1024 */
  deflate_threshold_bytes?: number
  /** 握手超时（ms）。默认: 15000 */
  handshake_timeout_ms?: number
  /** 最大 payload（字节）。默认: 64MB */
  max_payload_bytes?: number
  /** 重连配置 */
  reconnect?: ReconnectConfig
  /** 心跳配置 */
  heartbeat?: HeartbeatConfig
  /** 同时竞速端点数。默认: 1 */
  race_count?: number
  /** TLS 证书校验。默认: true */
  reject_unauthorized?: boolean
}

/** WebSocket 事件 — 所有回调参数的联合类型 */
export type WsEvent =
  | { type: 'Connected'; url: string; latency_ms: number }
  | { type: 'Disconnected'; code: number; reason: string }
  // data 为 base64 编码，字段名与 Rust WsEvent::to_ffi_json() 一致
  | { type: 'Message'; data_base64: string; is_binary: boolean }
  | { type: 'Error'; message: string }
  | { type: 'Reconnecting'; attempt: number; delay_ms: number }
  | { type: 'HeartbeatRtt'; rtt_ms: number }
```

#### 4.3.4 `ts/client.ts` — HTTP wrapper

```typescript
import type {
  HttpClientConfig,
  RequestOptions,
  HttpResponse,
  Metrics,
  StreamEvent,
} from './types'
import { loadNativeAddon } from './native'

const { JsHttpClient } = loadNativeAddon('catcher-napi-http')

/**
 * 类型安全的 HTTP 客户端
 *
 * ```ts
 * const client = new HttpClient({ base_url: 'https://api.example.com' })
 * const resp = await client.get('/users/1')
 * ```
 */
export class HttpClient {
  private _raw: any  // napi 原生 JsHttpClient 实例

  constructor(config: HttpClientConfig | string) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    this._raw = new JsHttpClient(json)
  }

  async get(url: string, options?: RequestOptions): Promise<HttpResponse> {
    return this._raw.get(url, options ?? undefined)
  }

  async post(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    return this._raw.post(url, body ?? undefined, options ?? undefined)
  }

  async put(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.put) {
      throw new Error('put() requires rebuilt native addon (cargo build)')
    }
    return this._raw.put(url, body ?? undefined, options ?? undefined)
  }

  async delete(url: string, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.delete) {
      throw new Error('delete() requires rebuilt native addon (cargo build)')
    }
    return this._raw.delete(url, options ?? undefined)
  }

  async patch(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse> {
    if (!this._raw.patch) {
      throw new Error('patch() requires rebuilt native addon (cargo build)')
    }
    return this._raw.patch(url, body ?? undefined, options ?? undefined)
  }

  circuitBreakerState(): 'closed' | 'open' | 'half-open' {
    return this._raw.circuitBreakerState()
  }

  metrics(): Metrics {
    return this._raw.metrics()
  }

  setAdaptiveTimeout(
    minTimeoutMs: number,
    maxTimeoutMs: number,
    multiplier: number,
    windowSize: number,
  ): void {
    this._raw.setAdaptiveTimeout(minTimeoutMs, maxTimeoutMs, multiplier, windowSize)
  }

  disableAdaptiveTimeout(): void {
    this._raw.disableAdaptiveTimeout()
  }

  cancelAll(): void {
    this._raw.cancelAll()
  }

  cancelRequest(requestId: number): boolean {
    return this._raw.cancelRequest(requestId)
  }

  nextRequestId(): number {
    return this._raw.nextRequestId()
  }

  /**
   * 流式下载 — 回调直接收到解析后的强类型事件对象
   */
  executeStream(
    method: string,
    url: string,
    body?: Buffer,
    options?: RequestOptions,
    onChunk?: (event: StreamEvent) => void,
  ): void {
    const wrapped = typeof onChunk === 'function'
      ? (eventJson: string) => {
          try {
            onChunk(JSON.parse(eventJson))
          } catch {
            onChunk({ type: 'Error', message: eventJson })
          }
        }
      : undefined

    this._raw.executeStream(method, url, body ?? undefined, options ?? undefined, wrapped)
  }
}
```

#### 4.3.5 `ts/sse.ts` — SSE wrapper

```typescript
import type { SseClientConfig, SseEvent } from './types'
import { loadNativeAddon } from './native'

const native = loadNativeAddon('catcher-napi-http')

function wrapSseCallback(onEvent: (event: SseEvent) => void): (eventJson: string) => void {
  return (eventJson: string) => {
    try {
      onEvent(JSON.parse(eventJson))
    } catch {
      onEvent({ type: 'Error', message: eventJson })
    }
  }
}

/** 一次性 SSE 流（无自动重连） */
export class SseStream {
  private _handle: any

  constructor(config: SseClientConfig | string, onEvent: (event: SseEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    this._handle = native.sseStream(json, wrapSseCallback(onEvent))
  }

  close(): void {
    this._handle.close()
  }
}

/** 长连接 SSE 客户端（自动重连） */
export class SseClient {
  private _handle: any

  constructor(config: SseClientConfig | string, onEvent: (event: SseEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)
    this._handle = native.sseClient(json, wrapSseCallback(onEvent))
  }

  close(): void {
    this._handle.close()
  }
}
```

#### 4.3.6 `ts/client.ts` — WebSocket wrapper

```typescript
import type { WsClientConfig, WsEvent } from './types'
import { loadNativeAddon } from './native'

const { JsWsClient } = loadNativeAddon('catcher-napi-ws')

/**
 * 类型安全的 WebSocket 客户端
 *
 * **注意**：不要在事件回调内同步调用 `send()`，否则可能因 napi 单线程
 * 限制导致死锁。如需在回调中发送消息，请使用 `setImmediate` 或
 * `process.nextTick` 延迟执行。
 *
 * ```ts
 * const ws = new WsClient(
 *   { urls: ['wss://echo.example.com'] },
 *   (event) => {
 *     if (event.type === 'Connected') console.log('Connected to', event.url)
 *   },
 * )
 * ws.send('hello')
 * ```
 */
export class WsClient {
  private _raw: any  // napi 原生 JsWsClient 实例

  constructor(config: WsClientConfig | string, onEvent?: (event: WsEvent) => void) {
    const json = typeof config === 'string' ? config : JSON.stringify(config)

    // 包装回调：自动 JSON.parse → 强类型
    const wrapped = typeof onEvent === 'function'
      ? (err: any, value: string) => {
          if (err) {
            onEvent({ type: 'Error', message: err.message ?? String(err) })
            return
          }
          if (typeof value === 'string') {
            try {
              onEvent(JSON.parse(value))
            } catch {
              onEvent({ type: 'Error', message: value })
            }
          }
        }
      : undefined

    this._raw = new JsWsClient(json, wrapped)
  }

  /** 发送文本消息 */
  send(data: string): void {
    this._raw.send(data)
  }

  /** 发送二进制消息 */
  sendBinary(data: Buffer | ArrayBuffer | Uint8Array): void {
    let buf: Buffer
    if (data instanceof ArrayBuffer) {
      buf = Buffer.from(data)
    } else if (data instanceof Uint8Array) {
      buf = Buffer.from(data.buffer, data.byteOffset, data.byteLength)
    } else {
      buf = data
    }
    this._raw.sendBinary(buf)
  }

  /** 关闭连接。默认 code=1000, reason='normal' */
  close(code?: number, reason?: string): void {
    this._raw.close(code ?? null, reason ?? null)
  }
}
```

### 4.4 构建配置

#### `tsconfig.json`

```jsonc
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "CommonJS",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true,
    "declaration": true,
    "outDir": "dist",
    "rootDir": "ts",
    "skipLibCheck": true
  },
  "include": ["ts"]
}
```

#### `package.json`

**napi-http**：

```jsonc
{
  "name": "@eric8810/catcher-napi-http",
  "main": "dist/client.js",
  "types": "dist/client.d.ts",
  "exports": {
    ".": {
      "types": "./dist/client.d.ts",
      "require": "./dist/client.js"
    },
    "./types": {
      "types": "./dist/types.d.ts",
      "require": "./dist/types.js"
    }
  },
  "files": [
    "dist/",
    "index.js",
    "index.d.ts",
    "*.node",
    "npm/"
  ],
  "scripts": {
    "build:ts": "tsup ts/client.ts ts/types.ts ts/sse.ts --format cjs --dts --outDir dist --clean",
    "build": "napi build --platform --release && npm run build:ts && napi artifacts",
    "build-debug": "cargo build",
    "typecheck": "tsc --noEmit",
    "prepublishOnly": "node -e \"const fs=require('fs');if(!fs.existsSync('dist/client.js')){console.error('Run npm run build first');process.exit(1)}\""
  },
  "devDependencies": {
    "@napi-rs/cli": "^2.18.0",
    "tsup": "^8.0.0",
    "typescript": "^5.0.0"
  }
}
```

**napi-ws**：同理，`build:ts` 入口为 `ts/client.ts ts/types.ts`（无 `sse.ts`）。

#### 为什么选 `tsup` 而不是 `tsc`

| 维度 | `tsc` | `tsup` |
|------|-------|--------|
| 输出 | 每个 `.ts` → 对应 `.js` + `.d.ts` | bundle 为单个 `.js` + `.d.ts` |
| 配置 | 需要 `tsconfig.json` paths 映射 | 零配置 |
| 速度 | 全量编译 | esbuild，快 10-20x |
| 发布体积 | 多文件 | bundle 后更紧凑 |

> **注意**：`native.ts` 不是 tsup 入口，而是被 `client.ts` 和 `sse.ts` 各自 import。tsup 对每个入口独立 bundle，`native.ts` 逻辑会被内联到 `client.js` 和 `sse.js` 各一份——运行时正确，`dist/` 中不会出现独立的 `native.js`。`types.ts` 只有 `export type`，编译后 `types.js` 为空文件，这是正常现象。

### 4.5 camelCase 兼容（Phase 2，需改 Rust）

Rust struct 各字段添加 `#[serde(alias)]`，同时接受 snake_case 和 camelCase：

```rust
// catcher-http/src/types/http.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientConfig {
    #[serde(alias = "baseUrl")]
    pub base_url: String,

    #[serde(alias = "connectTimeoutMs")]
    pub connect_timeout_ms: u64,

    #[serde(alias = "responseTimeoutMs")]
    pub response_timeout_ms: u64,
    // ...
}
```

零运行时开销，Rust 侧直接支持两种命名。影响范围：所有 Config struct 的所有字段。

---

## 5. 迁移策略

### 5.1 删除旧文件

| 删除 | 说明 |
|------|------|
| `packages/catcher-napi-http/client.js` | 被 `ts/client.ts` 替代 |
| `packages/catcher-napi-http/client.d.ts` | 由 `tsup` 自动生成 |
| `packages/catcher-napi-ws/client.js` | 被 `ts/client.ts` 替代 |
| `packages/catcher-napi-ws/client.d.ts` | 由 `tsup` 自动生成 |

### 5.2 保留文件

| 保留 | 说明 |
|------|------|
| `index.js` / `index.d.ts` | `napi build` 自动生成，作为底层原生绑定 |

### 5.3 `.gitignore`

```
packages/catcher-napi-http/dist/
packages/catcher-napi-ws/dist/
```

构建产物不入库，CI 中 `npm run build` 时生成。

---

## 6. 与 TS 类型包的关系

napi 包的配置类型是 Rust struct 的 **1:1 投影**，与 TS 纯 TS 包（`catcher-core-ts`）中的 `HttpClientConfig` 是**不同的类型**，不应强行统一：

| 维度 | `catcher-core-ts` | napi-http |
|------|-------------------|-----------|
| 字段命名 | camelCase（`baseURL`） | snake_case（`base_url`） |
| 功能范围 | 完整（含拦截器、adapter） | 子集（无拦截器等） |
| 值类型 | 回调函数、AbortSignal 等 | 仅 JSON 可序列化值 |
| 来源 | TS 手写 | `.ts` → `tsup` 自动生成 `.d.ts` |

理由：TS 包含 `interceptors`、`retry.retryIf` 等函数类型字段和 `AbortSignal`、`() => Promise<string>` 等值类型，napi 作为 Rust 薄封装无法支持。

---

## 7. 用法示例

```typescript
import { HttpClient, SseStream } from '@eric8810/catcher-napi-http'
import type { HttpClientConfig, SseEvent } from '@eric8810/catcher-napi-http'

// ── HTTP ──
const config: HttpClientConfig = {
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  retry: { max_attempts: 3, backoff: 'Exponential' },
}

const client = new HttpClient(config)
const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// ── SSE ──
const stream = new SseStream({ url: 'https://stream.example.com' }, (event: SseEvent) => {
  if (event.type === 'Line') console.log(event.data)
})
stream.close()

// ── WebSocket ──
import { WsClient } from '@eric8810/catcher-napi-ws'
import type { WsEvent } from '@eric8810/catcher-napi-ws'

const ws = new WsClient(
  { urls: ['wss://echo.example.com'] },
  (event: WsEvent) => {
    switch (event.type) {
      case 'Connected':
        console.log(event.url, event.latency_ms)
        break
      case 'Message':
        console.log(event.is_binary ? '(binary)' : event.data_base64)
        break
      case 'Reconnecting':
        console.log(`attempt ${event.attempt} in ${event.delay_ms}ms`)
        break
      case 'HeartbeatRtt':
        console.log(`RTT: ${event.rtt_ms}ms`)
        break
    }
  },
)
ws.send('hello')
ws.close()
```

---

## 8. 变更清单

### Phase 1 — TypeScript wrapper 重写（核心，低风险，不改 Rust）

| 文件 | 变更 |
|------|------|
| `packages/catcher-napi-http/ts/types.ts` | **新建** — HTTP/SSE 配置和事件类型 |
| `packages/catcher-napi-http/ts/native.ts` | **新建** — 原生模块加载 |
| `packages/catcher-napi-http/ts/client.ts` | **新建** — HttpClient wrapper |
| `packages/catcher-napi-http/ts/sse.ts` | **新建** — SseStream / SseClient wrapper |
| `packages/catcher-napi-http/tsconfig.json` | **新建** |
| `packages/catcher-napi-http/package.json` | **修改** — 入口指向 `dist/`，添加 `tsup` 依赖 |
| `packages/catcher-napi-http/client.js` | **删除** |
| `packages/catcher-napi-http/client.d.ts` | **删除** |
| `packages/catcher-napi-ws/ts/types.ts` | **新建** — WS 配置和事件类型 |
| `packages/catcher-napi-ws/ts/native.ts` | **新建** |
| `packages/catcher-napi-ws/ts/client.ts` | **新建** — WsClient wrapper |
| `packages/catcher-napi-ws/tsconfig.json` | **新建** |
| `packages/catcher-napi-ws/package.json` | **修改** |
| `packages/catcher-napi-ws/client.js` | **删除** |
| `packages/catcher-napi-ws/client.d.ts` | **删除** |

### Phase 2 — camelCase 兼容（需改 Rust，中等风险）

| 文件 | 变更 |
|------|------|
| `packages/catcher-http/src/types/http.rs` | `HttpClientConfig` 及子结构体添加 `#[serde(alias)]` |
| `packages/catcher-ws/src/types/ws.rs` | `WsClientConfig` 及子结构体添加 `#[serde(alias)]` |
| `packages/catcher-core/src/types/sse.rs` | `SseClientConfig` 添加 `#[serde(alias)]` |
| `packages/catcher-core/src/types/resilience.rs` | `RetryConfig` / `CircuitBreakerConfig` 添加 `#[serde(alias)]` |
| `ts/types.ts` | JSDoc 注释中标注 camelCase 别名 |

### Phase 3 — CI 集成

| 文件 | 变更 |
|------|------|
| `.github/workflows/ci.yml` | 添加 `pnpm build:ts` 步骤 |
| `.github/workflows/release.yml` | 确保 `build:ts` 在 napi 构建后执行 |
| `.gitignore` | 添加 `packages/catcher-napi-*/dist/` |

### Phase 4 — Rust 代码规范清理（与 Phase 2 同步进行）

| 问题 | 文件 | 变更 |
|------|------|------|
| `default_true()` 重复定义 | `catcher-http/src/types/http.rs`、`catcher-ws/src/types/ws.rs`、`catcher-core/src/types/resilience.rs` | 统一从 `catcher-core` 公共模块导入（AGENTS.md 规定） |
| SSE `serde_json::json!` 手动构建 | `catcher-napi-http/src/sse.rs:179-187` | 改为 tagged enum `#[serde(tag = "type")]` + derive 序列化（RUST_STYLE_GUIDE 规定） |
| `RetryConfig.backoff` 默认值不一致 | `catcher-core/src/types/resilience.rs:23-24` vs `:56` | `#[serde(default)]` 用 `BackoffKind::default()`（Fixed），`RetryConfig::default()` 显式写 `Exponential`，两者应对齐 |
