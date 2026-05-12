# 00 — TS 包概览

> v0.2.0 — 与 Rust workspace 对齐，codec 合并到 WS

## 依赖关系

```
            @catcher/core (zero deps)
            /            \
           /              \
  @catcher/http      @catcher/ws
                     (内置 msgpack)
```

## 按场景安装

```bash
# REST API
npm i @catcher/http

# IM 实时通信
npm i @catcher/http @catcher/ws

# 文件上传
npm i @catcher/http
```

## 使用方式

```typescript
// HTTP
import { createHttpClient } from '@catcher/http'
const client = createHttpClient({ baseURL: 'https://api.example.com' })

// WebSocket
import { createResilientWS } from '@catcher/ws'
const ws = createResilientWS({
  url: 'wss://ws.example.com',
  codec: 'msgpack',  // 开关：'json' (默认) | 'msgpack'
})
ws.send({ event: 'message', data: 'hello' })  // 内部自动 pack

// 类型
import type { HttpClientConfig, ResilientWSOptions } from '@catcher/core'
```
