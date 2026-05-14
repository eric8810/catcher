# 01 — 包配置 (v0.3)

> 代码位置：`packages/catcher-*-ts/`

## @eric8810/catcher-core

```json
{ "name": "@eric8810/catcher-core", "dependencies": {} }
```
零运行时依赖，纯 TypeScript 类型导出。

## @eric8810/catcher-http

**依赖**：`@eric8810/catcher-core`, `cacheable-lookup`, `cockatiel`, `p-retry`, `p-queue`  
**Optional peer**：`axios`  
**导出**：`createHttpClient`, `createRetryWrapper`, `createSharedAgent`, `clearDnsCache`, `createPriorityQueue`, `enqueueWithPriority`, **`createSSEClient`**, **`createSSEStream`**

## @eric8810/catcher-ws

**依赖**：`@eric8810/catcher-core`  
**Optional peer**：`ws`, `msgpackr`  
**导出**：`createResilientWS`, `createReconnectStrategy`, `raceEndpoints`, `pack`, `unpack`, `isBinary`, `decodeWSMessage`

## @eric8810/catcher-web

**依赖**：`@eric8810/catcher-core`, `cockatiel`, `p-retry`, `p-queue`  
**导出**：`createHttpClient`, **`createSSEClient`**, **`createSSEStream`**

> SSE 模块零新增依赖 — Node.js 18+ 和浏览器原生支持 `fetch` + `ReadableStream`。
