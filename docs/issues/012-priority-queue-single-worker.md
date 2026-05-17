# Performance: Priority Queue 单 worker 任务分发瓶颈

**严重程度**: 🟡 Low — 极高吞吐场景下 worker 成为瓶颈

**状态**: Open

**位置**: `packages/catcher-http/src/scheduler/priority_queue.rs:39-70`

---

## 当前代码

```rust
tokio::spawn(async move {
    loop {
        let task = tokio::select! {
            biased;
            t = high_rx.recv() => t,
            t = low_rx.recv() => t,
        };
        let Some(task) = task else { break; };

        let s = sem_clone.clone();
        tokio::spawn(async move {
            let _permit = s.acquire().await;
            let result = tokio::time::timeout(...).await;
            let _ = task.respond_to.send(response);
        });
    }
});
```

## 问题

所有任务通过 **单个** tokio task 从两个 mpsc channel 接收后分发。这个 worker 是串行瓶颈：

1. 任务提交速率 > worker 处理速率时，任务在 channel 中堆积
2. 每个任务分发包括 `sem_clone.clone()` + `tokio::spawn()` — 虽然很快，但在串行循环中执行
3. `biased` select 确保高优先级先处理，但低优先级任务可能在 channel 中饥饿

**实际影响**：对于 HTTP 请求场景（毫秒级延迟），单 worker 足以处理每秒数千个任务的入队/分发。但在极端场景（微秒级任务、大量 burst）中成为瓶颈。

## 修复方案

### 方案 A：批量接收

```rust
tokio::spawn(async move {
    loop {
        // 批量接收再分发
        let mut batch = Vec::with_capacity(32);
        // 先收 high，再收 low（保持优先级）
        while let Ok(task) = high_rx.try_recv() {
            batch.push(task);
            if batch.len() >= 32 { break; }
        }
        if batch.is_empty() {
            while let Ok(task) = low_rx.try_recv() {
                batch.push(task);
                if batch.len() >= 32 { break; }
            }
        }
        if batch.is_empty() {
            // fallback to blocking recv
            let task = tokio::select! { biased; ... };
            batch.push(task);
        }
        for task in batch {
            let s = sem_clone.clone();
            tokio::spawn(async move { ... });
        }
    }
});
```

### 方案 B：多 worker 并行

使用 `tokio::sync::mpsc` 的多消费者特性（但 mpsc 不保证优先级）。

## 关联

- `PriorityRequestQueue` 设计用于 A-01（并发控制 + 优先级）
- 之前报告：`005-config-clone-per-request.md`
