# Bug: CircuitBreaker `on_success` / `on_failure` TOCTOU 锁竞态

**严重程度**: 🟡 Medium — 高并发下状态迁移可能不正确

**状态**: Open

**位置**: `packages/catcher-http/src/resilience/circuit_breaker.rs:70-113`

---

## 当前代码

```rust
pub fn on_success(&self) {
    let state = *self.state.lock();  // ← 锁 → 读 → 释放
    match state {
        CbState::HalfOpen => {
            let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.config.success_threshold {
                *self.state.lock() = CbState::Closed;  // ← 再次锁 → 写 → 释放
                // ...
            }
        }
        // ...
    }
}
```

## 问题

`on_success()` 和 `on_failure()` 都采用"锁-读-释放"然后"锁-写-释放"的两阶段模式。在两次加锁之间有竞争窗口：

**场景 A（HalfOpen → Closed 竞态）**：
1. 线程 A 读 state = HalfOpen，`success_count` 达到 threshold-1
2. 线程 B 读 state = HalfOpen，`success_count` 也达到 threshold-1
3. A、B 都判定 `count >= success_threshold`，都执行 `*state.lock() = Closed`
4. 但此时可能已经有新的 failure 到来，应该让状态回到 Open

**场景 B（Closed → Open 竞态）**：
1. 线程 A 读 state = Closed，`failure_count` = threshold - 1
2. 线程 A `fetch_add` → threshold，判定需要 open
3. 但在 A 获取写锁之前，线程 B 调用了 `on_success()` 重置了 `failure_count`
4. A 仍然执行 `state = Open`，但此时 `failure_count` 已被 B 重置

此外，两次加锁的模式增加了不必要的锁开销——每次热路径调用多一次 `parking_lot::Mutex::lock()`。

## 修复

在整个方法期间持有锁：

```rust
pub fn on_success(&self) {
    let mut state = self.state.lock();
    match *state {
        CbState::Closed => {
            self.failure_count.store(0, Ordering::Relaxed);
        }
        CbState::HalfOpen => {
            let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.config.success_threshold {
                *state = CbState::Closed;
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
            }
        }
        CbState::Open => {}
    }
}
```

或者改为完全无锁的 Atomic 状态机（用 `AtomicU8` 编码状态并用 CAS 转换）。

## 关联

- 之前报告：`006-handle-registry-lock-contention.md`（锁竞争）
- 规范：AGENTS.md 并发规则 "状态机字段 → `AcqRel`"
