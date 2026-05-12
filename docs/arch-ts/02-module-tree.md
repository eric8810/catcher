# 02 — 模块树 (v0.2)

> 代码位置：`packages/catcher-*-ts/`

## @catcher/core

```
catcher-core-ts/
├── package.json
└── src/
    ├── index.ts
    └── types.ts
```

## @catcher/http

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
    └── queue/
        ├── index.ts
        └── priority-queue.ts
```

## @catcher/ws

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
