# 01 — 包配置 (v0.2)

> 代码位置：`packages/catcher-*-ts/`

## @catcher/core

```json
{ "name": "@catcher/core", "dependencies": {} }
```
零运行时依赖，纯 TypeScript 类型导出。

## @catcher/http

**依赖**：`@catcher/core`, `cacheable-lookup`, `cockatiel`, `p-retry`, `p-queue`  
**Optional peer**：`axios`  
**导出**：`createHttpClient`, `createRetryWrapper`, `createSharedAgent`, `clearDnsCache`, `createPriorityQueue`, `enqueueWithPriority`

## @catcher/ws

**依赖**：`@catcher/core`  
**Optional peer**：`ws`, `msgpackr`  
**导出**：`createResilientWS`, `createReconnectStrategy`, `raceEndpoints`, `pack`, `unpack`, `isBinary`, `decodeWSMessage`
