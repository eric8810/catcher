# Bug: `catcher_evaluate_quality` 并发调用 panic —— take/put 模式竞态

**严重程度**: 🔴 High — 两个并发调用同时 take evaluator，第二个 panic

**状态**: Open

**位置**: `packages/catcher-http/src/ffi/quality_ffi.rs:56-75`

---

## 当前代码

```rust
runtime().spawn(async move {
    // Step 1: check if evaluator exists
    {
        let mut guard = EVALUATOR.lock().unwrap();
        if guard.is_none() {
            *guard = Some(NetworkQualityEvaluator::new(50));
        }
    }

    // Step 2-4: take → use → put back
    let result = {
        let mut evaluator = EVALUATOR.lock().unwrap().take().unwrap();  // PANICS here!
        let result = evaluator.measure_http_rtt(&host_str, "/").await;
        EVALUATOR.lock().unwrap().replace(evaluator);
        result
    };
    // ...
});
```

## 竞态场景

两个并发调用 A 和 B：

| 时间 | 调用 A | 调用 B |
|------|--------|--------|
| T1 | Step 1: EVALUATOR = Some | Step 1: EVALUATOR = Some |
| T2 | Step 2: `take()` → 拿到 evaluator | — |
| T3 | — | Step 2: `take()` → **PANIC** (slot 是 None) |

两个调用都在 `runtime().spawn()` 内执行，完全可能并发。

## 修复

持锁覆盖整个 take→use→put 周期，或使用 `Arc<Mutex<NetworkQualityEvaluator>>` 替代 `take/put`：

```rust
static EVALUATOR: std::sync::Mutex<NetworkQualityEvaluator> = 
    std::sync::Mutex::new(NetworkQualityEvaluator::new(50));  // eager init

runtime().spawn(async move {
    let result = {
        let mut evaluator = EVALUATOR.lock().unwrap();
        evaluator.measure_http_rtt(&host_str, "/").await  // hold lock across await
    };
    // ...
});
```

但 `measure_http_rtt` 是 async，持 `std::sync::MutexGuard` 跨 `.await` 违反 Rust 规则。替代方案：用 `tokio::sync::Mutex`。

## 附加：同函数第二个 panic 点

```rust
let mut guard = EVALUATOR.lock().unwrap();
let evaluator = guard.as_mut().unwrap();  // ← 也可能 panic
```

take/put 之间的时间窗口内，另一个调用也可能再次 take，导致此处 `unwrap()` 同样 panic。

## 关联

- 同样问题出现在 uniffi `evaluate_quality`（`uniffi/lib.rs:597-599,601-602`）— 使用相同的 take/put 模式，两处 panic 点
