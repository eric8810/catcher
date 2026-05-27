# 03 — 类型系统

> 对应源文件：`src/types.ts`（443 行）

## SharedAgentOptions — Agent 配置

```typescript
interface SharedAgentOptions {
  keepAlive?: boolean           // 默认 true
  keepAliveMsecs?: number       // 默认 30_000
  maxSockets?: number           // 默认 25
  maxFreeSockets?: number       // 默认 10
  timeout?: number              // 默认 60_000
  rejectUnauthorized?: boolean  // 默认 false
  dnsCacheTtl?: number          // 默认 300
}
```

## HttpClientConfig — HTTP 客户端配置

```typescript
interface HttpClientConfig {
  baseURL: string
  keepAlive?: boolean
  dnsCacheTtl?: number
  rejectUnauthorized?: boolean
  timeout?: { connect?: number; response?: number } | number
  retry?: {
    attempts: number
    backoff?: 'fixed' | 'exponential'
    retryIf?: (error: any) => boolean
    onRetry?: (attempt: number) => void
  }
  concurrency?: number
  circuitBreaker?: {
    failureThreshold: number
    resetTimeout: number
  }
  interceptors?: {
    request?: Array<(config: any) => any>
    response?: Array<(response: any) => any>
  }
}
```

## IHttpClient — HTTP 客户端接口

```typescript
interface IHttpClient {
  get<T>(url: string, config?: Record<string, any>): Promise<T>
  post<T>(url: string, body?: any, config?: Record<string, any>): Promise<T>
  put<T>(url: string, body?: any, config?: Record<string, any>): Promise<T>
  delete<T>(url: string, config?: Record<string, any>): Promise<T>
  patch<T>(url: string, body?: any, config?: Record<string, any>): Promise<T>
}
```

## ResilientWSOptions — WebSocket 配置

```typescript
interface ResilientWSOptions {
  url: string | string[]
  protocol?: string | string[]
  perMessageDeflate?: boolean | { threshold?: number }
  handshakeTimeout?: number     // 默认 10_000
  maxPayload?: number           // 默认 1MB
  reconnect?: {
    initialDelay?: number       // 默认 1000
    maxDelay?: number           // 默认 30_000
    backoffMultiplier?: number  // 默认 2
    maxAttempts?: number        // 默认 20
  }
  raceCount?: number            // 默认 3
  headers?: Record<string, string>
  rejectUnauthorized?: boolean
}
```

## ResilientWS — WebSocket 实例接口

```typescript
interface ResilientWS extends EventTarget {
  send(data: string | Buffer): void
  close(code?: number, reason?: string): void
  readonly readyState: number
  readonly url: string
  readonly status: 'CONNECTING' | 'CONNECTED' | 'CLOSED'
  addEventListener(type: 'open' | 'close' | 'message' | 'error', listener: EventListener): void
  removeEventListener(type: string, listener: EventListener): void
}
```

## RetryOptions — 重试配置

```typescript
interface RetryOptions {
  attempts: number
  backoff?: 'fixed' | 'exponential'
  minTimeout?: number           // 默认 500
  maxTimeout?: number           // 默认 30_000
  onRetry?: (attemptNum: number) => void
}
```

## PriorityQueueOptions — 队列配置

```typescript
interface PriorityQueueOptions {
  concurrency?: number          // 默认 10
  timeout?: number              // 默认无超时
}
```
