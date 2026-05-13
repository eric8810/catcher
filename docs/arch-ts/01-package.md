# 01 — 包配置 (v0.2)

> 代码位置：`packages/catcher-*-ts/`

## @eric8810/core

```json
{ "name": "@eric8810/core", "dependencies": {} }
```
零运行时依赖，纯 TypeScript 类型导出。

## @eric8810/http

**依赖**：`@eric8810/core`, `cacheable-lookup`, `cockatiel`, `p-retry`, `p-queue`  
**Optional peer**：`axios`  
**导出**：`createHttpClient`, `createRetryWrapper`, `createSharedAgent`, `clearDnsCache`, `createPriorityQueue`, `enqueueWithPriority`

## @eric8810/ws

**依赖**：`@eric8810/core`  
**Optional peer**：`ws`, `msgpackr`  
**导出**：`createResilientWS`, `createReconnectStrategy`, `raceEndpoints`, `pack`, `unpack`, `isBinary`, `decodeWSMessage`
