# 08 — 优先级请求队列

> 对应源文件：`src/queue/priority-queue.ts`（28 行）

## 职责

提供基于优先级的请求调度：
- 限制最大并发请求数
- 低优先级数字 = 高优先级（0 最高）
- 可选队列超时

## 核心导出

```typescript
import { createPriorityQueue, enqueueWithPriority } from 'catcher/queue'
```

### createPriorityQueue(options?) → PQueue

```typescript
function createPriorityQueue(options?: PriorityQueueOptions): PQueue

interface PriorityQueueOptions {
  concurrency?: number   // 最大并发，默认 10
  timeout?: number        // 队列超时毫秒，默认无超时
}
```

返回 `p-queue` 实例。

### enqueueWithPriority(queue, priority, fn) → Promise

```typescript
function enqueueWithPriority(
  queue: PQueue,
  priority: number,     // 越小越优先
  fn: () => Promise<any>,
): Promise<any>
```

## 预定义优先级

| 常量 | 值 | 场景 |
|------|-----|------|
| 0 | 最高 | 消息发送 / 已读上报 |
| 3 | 高 | 消息列表加载 |
| 5 | 普通 | 用户信息 / 频道信息 |
| 7 | 低 | 头像加载 |
| 10 | 最低 | 表情 / 预加载 / 配置 |

## 在 HTTP 客户端中的集成

```typescript
// client.ts 中的优先级映射
get(url, config)    → priority: 3  // 读操作低优
post(url, body)     → priority: 1  // 写操作优先
put(url, body)      → priority: 2
delete(url, config) → priority: 3
patch(url, body)    → priority: 2
```

## 实现细节

底层使用 `p-queue` 库。`throwOnTimeout: true` 使超时任务抛出 `TimeoutError`。
