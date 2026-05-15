# 错误处理指南

> catcher 提供结构化的错误类型体系，帮助开发者精确区分网络故障类型并做针对性处理。

---

## 一、错误类型分类

### CatcherErrorType

| 类型 | 含义 | 触发条件（Node.js） | 触发条件（浏览器） |
|------|------|-------------------|-------------------|
| `timeout` | 连接或响应超时 | `ECONNABORTED`, `ETIMEDOUT` | — |
| `connection` | 连接被拒绝/无法连接 | `ECONNREFUSED` | `TypeError: Failed to fetch` |
| `dns` | DNS 解析失败 | `ENOTFOUND` | — |
| `tls` | TLS/证书错误 | `UNABLE_TO_VERIFY_LEAF_SIGNATURE` 等 | — |
| `http` | HTTP 错误（4xx/5xx） | 有 `error.response` | 有 `error.response` |
| `cancelled` | 请求被取消 | `CanceledError`, `ERR_CANCELED` | `AbortError` |
| `unknown` | 其他未分类错误 | 以上均不匹配 | 以上均不匹配 |
| `SSE_TIMEOUT` | SSE 流无数据超时 | `createSSEStream` 内超时 | 同 |

---

## 二、CatcherHttpError 结构

```typescript
interface CatcherHttpError extends Error {
  readonly type: CatcherErrorType      // 分类标签
  readonly request: {                  // 完整的请求上下文
    method: string
    url: string
    headers: Record<string, string>
    config: RequestConfig
  }
  readonly response?: {                // 如果有 HTTP 响应
    status: number
    headers: Record<string, string>
    data: unknown
  }
  readonly attempt: number            // 已尝试次数（含重试）
  readonly elapsedMs: number          // 总耗时（毫秒）

  toJSON(): Record<string, unknown>   // 安全序列化（敏感头已脱敏）
}
```

### 用法示例

```typescript
import { createHttpClient } from '@eric8810/catcher-http'
import { isCatcherError } from '@eric8810/catcher-core'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
})

try {
  await client.get('/users/1')
} catch (error) {
  if (!isCatcherError(error)) {
    // 非 catcher 错误 — 可能是代码 bug
    throw error
  }

  switch (error.type) {
    case 'timeout':
      console.error(`请求超时 — 耗时 ${error.elapsedMs}ms, 尝试 ${error.attempt} 次`)
      // 可以降级到缓存数据或重试
      break

    case 'connection':
      console.error(`无法连接到 ${error.request.url}`)
      // 网络断开或服务端不可达
      break

    case 'dns':
      console.error(`DNS 解析失败 — 检查网络或 nameserver 配置`)
      break

    case 'tls':
      console.error(`TLS 错误 — 证书过期或自签名`)
      break

    case 'http':
      console.error(
        `HTTP ${error.response!.status}`,
        `— ${error.request.method} ${error.request.url}`,
        `— 耗时 ${error.elapsedMs}ms`
      )
      if (error.response!.status >= 500) {
        // 服务端错误，重试通常值得
      } else if (error.response!.status === 429) {
        // 频率限制，等待后重试
      } else if (error.response!.status === 401) {
        // 认证失败，刷新 token
        await refreshToken()
        return client.get('/users/1')
      }
      break

    case 'cancelled':
      console.log('请求已取消')
      break

    default:
      console.error('未分类错误:', error.message)
  }
}
```

---

## 三、重试决策

### 哪些错误应该重试

| 类型 | 是否重试 | 原因 |
|------|:------:|------|
| `timeout` | ✅ | 可能是网络波动 |
| `connection` | ✅ | 实例重启或临时不可达 |
| `dns` | ✅ | DNS 服务暂时不可用 |
| `tls` | ❌ | 证书问题重试无意义 |
| `http` 5xx | ✅ | 服务端临时故障 |
| `http` 4xx | ❌ | 客户端错误（除 429） |
| `cancelled` | ❌ | 用户主动取消 |
| `unknown` | ⚠️ | 保守起见不重试 |

catcher 默认重试策略：
- TS 版：`ECONNRESET`, `ETIMEDOUT`, `ENOTFOUND`, `ECONNREFUSED`, 5xx
- Rust 版：`ErrorCategory::Retryable` 的错误（timeout / connection / dns / 5xx）

### 自定义重试条件

```typescript
const client = createHttpClient({
  baseURL: 'https://api.example.com',
  retry: {
    attempts: 3,
    retryIf: (error) => {
      // 自定义：只在特定条件下重试
      if (isCatcherError(error)) {
        return error.type === 'timeout' && error.attempt < 2
      }
      return false
    },
  },
})
```

---

## 四、Rust 错误处理

```rust
use catcher_core::{CatcherError, ErrorCategory};

match error {
    CatcherError::ConnectionTimeout(ms) => {
        tracing::warn!("连接超时 {}ms", ms);
        // 可重试
    }
    CatcherError::ConnectionRefused => {
        tracing::error!("连接被拒绝，可能服务未启动");
    }
    CatcherError::DnsResolutionFailed(host) => {
        tracing::error!("DNS 解析失败: {}", host);
    }
    CatcherError::TlsError(msg) => {
        tracing::error!("TLS 错误: {}", msg);
    }
    CatcherError::HttpError { status, message } => {
        if status >= 500 {
            tracing::warn!("服务端错误 {}: {}", status, message);
        } else {
            tracing::error!("客户端错误 {}: {}", status, message);
        }
    }
    CatcherError::CircuitBreakerOpen => {
        tracing::warn!("熔断器开启，请求被拦截");
        // 退避到降级逻辑
    }
    CatcherError::RetryExhausted { attempts, last_error } => {
        tracing::error!("重试 {} 次后仍失败: {}", attempts, last_error);
    }
    CatcherError::EncodeError(msg) | CatcherError::DecodeError(msg) => {
        tracing::error!("编解码错误: {}", msg);
    }
    CatcherError::InvalidConfig(msg) => {
        tracing::error!("配置错误: {}", msg);
        // 不应重试
    }
    CatcherError::Cancelled => {
        tracing::debug!("请求已取消");
    }
    _ => {
        tracing::error!("未知错误: {}", error);
    }
}

// 检查是否应该重试
match error.category() {
    ErrorCategory::Retryable => { /* 可以重试 */ }
    ErrorCategory::NonRetryable => { /* 不应重试 */ }
    ErrorCategory::Fatal => { /* 致命错误，配置问题 */ }
}
```

---

## 五、SSE 错误处理

```typescript
import { createSSEStream, createSSEClient } from '@eric8810/catcher-http'

// 一次性流
const stream = createSSEStream({ url: '...', timeout: 30_000 })

try {
  for await (const line of stream) {
    // 处理数据
  }
} catch (error) {
  if (error.type === 'SSE_TIMEOUT') {
    console.error('SSE 流超时 — 服务器可能卡住或网络中断')
  } else {
    console.error('SSE 连接失败:', error.message)
  }
}

// 长连接（自动重连，内部消化大部分错误）
const client = createSSEClient({
  url: 'https://api.example.com/events',
  reconnect: { maxRetries: 50 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

try {
  for await (const line of client) {
    // 处理数据
  }
} catch (error) {
  // 仅在 maxRetries 超过后才会抛出
  console.error('SSE 完全失败:', error)
}
```

---

## 六、最佳实践

### 1. 总是用 `isCatcherError` 做类型守卫

```typescript
// ✅ 好
if (isCatcherError(error)) {
  logToService(error.toJSON())
}

// ❌ 不好
if (error instanceof Error) {
  // 丢失了 type/attempt/elapsedMs 信息
}
```

### 2. 区分瞬时故障和永久故障

```typescript
function isTransient(error: CatcherHttpError): boolean {
  return error.type === 'timeout' ||
         error.type === 'connection' ||
         error.type === 'dns' ||
         (error.type === 'http' && error.response!.status >= 500)
}
```

### 3. 安全日志（敏感头自动脱敏）

```typescript
// toJSON 自动脱敏 Authorization/Cookie/Proxy-Authorization
logger.error('Request failed', error.toJSON())

// 不要直接序列化 error.request.headers — 包含 token
console.log(JSON.stringify(error.request.headers))  // ❌
console.log(JSON.stringify(error.toJSON()))          // ✅
```

### 4. 熔断器状态监控

```typescript
setInterval(() => {
  const state = client.circuitBreakerState()
  if (state === 'open') {
    console.warn('熔断器开启 — 所有请求被拦截')
    // 触发告警
  }
}, 5000)
```

### 5. 重试告警阈值

```typescript
client.on('retry', ({ attempt, error, url }) => {
  if (attempt >= 3) {
    console.warn(`重试已达 ${attempt} 次 — ${url}`, error)
    // 超过阈值才告警
  }
})
```
