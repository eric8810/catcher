# Performance: P90 计算每次克隆并排序整个窗口

**严重程度**: 🟡 Low — 在窗口较小时影响有限，高 QPS 下会成为热点

**状态**: Open

**位置**:

| 函数 | 文件 | 行号 |
|------|------|------|
| `AdaptiveTimeout::timeout_ms()` | `packages/catcher-http/src/resilience/timeout.rs` | 40-53 |
| `AdaptiveTimeout::snapshot()` | `packages/catcher-http/src/resilience/timeout.rs` | 61-86 |
| `NetworkQualityEvaluator::rtt_snapshot()` | `packages/catcher-http/src/observability/network_quality.rs` | 73-98 |
| `HeartbeatManager::p90_rtt()` | `packages/catcher-ws/src/ws/heartbeat.rs` | 72-81 |

---

## 模式

```rust
pub fn timeout_ms(&self) -> u64 {
    if self.rtt_window.is_empty() {
        return self.min_timeout_ms;
    }
    // ...
    let mut sorted = self.rtt_window.clone();   // ← 每次调用克隆整个 Vec
    sorted.sort_unstable();                      // ← O(n log n)
    let p90 = sorted[p90_idx];
    // ...
}
```

## 问题

每次 HTTP 请求调用 `timeout_ms()`、每次心跳调用 `p90_rtt()` 都会克隆并排序整个滑动窗口（默认 10-50 个元素）。在高 QPS 下，O(n log n) 乘以请求数成为不必要的开销。

窗口数据实际变化缓慢（只有 `record()` 写入时变化），P90 不需要每次重新排序。

## 建议方案

### 方案 A：惰性重算（推荐）

```rust
struct AdaptiveTimeout {
    rtt_window: Vec<u64>,
    cached_p90: Option<u64>,
    dirty: bool,
    // ...
}

pub fn record(&mut self, rtt_ms: u64) {
    // ...
    self.dirty = true;
}

pub fn timeout_ms(&mut self) -> u64 {
    if self.dirty {
        // 只在窗口变化后重算一次
        let mut sorted = self.rtt_window.clone();
        sorted.sort_unstable();
        self.cached_p90 = Some(sorted[p90_idx]);
        self.dirty = false;
    }
    // 直接返回缓存值
}
```

### 方案 B：二叉堆

用两个二叉堆（最大堆 + 最小堆）维护 P90，插入 O(log n)，查询 O(1)。

## 关联

- `timeout_ms()` 是请求热路径（每 HTTP 请求一次）
- `p90_rtt()` 是心跳热路径（每 heartbeat interval 一次）
- `rtt_snapshot()` 调用频率较低（仅 quality evaluation 时）
