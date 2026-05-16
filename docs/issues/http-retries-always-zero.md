# Issue: `http_retries` 指标在生产环境中永远为 0

**严重程度**: 🟡 中

---

## 根因

`MetricsCollector` 有 `http_retries: AtomicU64` 字段，`record_http_request(success, latency_us, retried)` 第三个参数 `retried` 控制是否递增。但 `http_client.rs` 全部 16 处调用都硬编码 `retried: false`（行 207, 241, 246, 257, 262, 271, 276, 286, 291, 311, 317）。

更深层原因：重试由 `reqwest-retry` 的 `RetryTransientMiddleware` 在 middleware 层内部执行，而该 middleware：

- **无** `on_retry` 回调
- **无** Response Extensions 写入 retry count
- **无** Response Header 附加 retry count
- 仅在**失败**时包装 `RetryError::WithRetries { retries, err }`，成功路径不暴露 retry 次数

`RetryTransientMiddleware::execute_with_retry()` 中的 `n_past_retries` 完全封闭在内部循环，外部无法感知。

## 调查证据

### MetricsCollector 定义
- `metrics.rs:12` — `http_retries: AtomicU64`
- `metrics.rs:37-48` — `retried: bool` → `fetch_add(1)` if true
- `metrics.rs:108` — snapshot 导出 `http_retries`

### 调用侧全为 false
- `http_client.rs` 所有 `record_http_request` 调用第 3 参数均为 `false`

### reqwest-retry v0.9.1 源码
- `middleware.rs:137-201` — retry 循环完全自包含
- `middleware.rs:188-200` — 仅 `RetryError::WithRetries` 暴露 retry 次数
- 无任何 hook / callback / extension 机制

## 修复方案

实现自定义 `MetricsRetryMiddleware`，替代 `RetryTransientMiddleware`：

```
MetricsRetryMiddleware
├── 复用 reqwest-retry 的 retry_policy + retryable_strategy 逻辑
├── 持有 Arc<AtomicU64> 指向 MetricsCollector::http_retries
├── 每次实际 retry 时递增 counter
└── 替换 RetryTransientMiddleware::new_with_policy(policy)
    → MetricsRetryMiddleware::new(policy, retries_counter)
```

### 改动清单

1. **`metrics.rs`** — 新增 `http_retries_ptr()` 返回 `Arc<AtomicU64>`，或直接暴露 `increment_http_retries()` 方法
2. **`http_client.rs`** — 新建 `MetricsRetryMiddleware`（可放在独立文件或 inline），替换 `RetryTransientMiddleware`
3. **清理** — `record_http_request` 的 `retried` 参数可删除（retry 计数由 middleware 负责）

### 替代方案（放弃）

- ~~拦截 `RetryError::WithRetries`~~ — 仅失败路径有效，成功重试无法计数
- ~~fork reqwest-retry 加 callback~~ — 引入维护负担
- ~~利用 `RetryableStrategy`~~ — 该 trait 只负责判断是否可重试，无重试回调

## 关联

- `s5-missing-retry.md` — retry 配置缺失
- `retry-over-triggers.md` — retry 过度触发延迟放大
