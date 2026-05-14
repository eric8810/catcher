# 02 — 模块树 (v0.3)

> 代码位置：`packages/catcher-*-ts/`

## @eric8810/catcher-core

```
catcher-core-ts/
├── package.json
└── src/
    ├── index.ts
    └── types.ts
```

## @eric8810/catcher-http

```
catcher-http-ts/
├── package.json
└── src/
    ├── index.ts
    ├── http/
    │   ├── index.ts
    │   ├── client.ts
    │   ├── retry.ts
    │   └── interceptors.ts       # 动态拦截器管理器
    ├── agent/
    │   ├── index.ts
    │   └── shared-agent.ts
    ├── queue/
    │   ├── index.ts
    │   └── priority-queue.ts
    └── sse/
        ├── index.ts              # 导出 createSSEStream, createSSEClient
        ├── router.ts             # SSE 行路由（~30 行）
        ├── stream.ts             # SSEStream — 一次性流式请求
        └── client.ts             # SSEClient — 长连接 + 自动重连
```

## @eric8810/catcher-ws

```
catcher-ws-ts/
├── package.json
└── src/
    ├── index.ts
    ├── codec.ts                   # msgpack 编解码（内置）
    └── ws/
        ├── index.ts
        ├── client.ts
        ├── reconnect.ts
        └── multi-endpoint.ts
```

## @eric8810/catcher-web

```
catcher-web/
├── package.json
├── tsconfig.json
└── src/
    ├── index.ts
    ├── http/
    │   └── client.ts               # fetch-based HTTP client
    └── sse/
        ├── index.ts                 # 导出 createSSEStream, createSSEClient
        ├── router.ts                # 同 catcher-http-ts 版
        ├── stream.ts                # 浏览器版 SSEStream
        └── client.ts                # 浏览器版 SSEClient
```
