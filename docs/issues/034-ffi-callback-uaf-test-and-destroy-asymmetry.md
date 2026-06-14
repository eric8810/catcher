# Bug: FFI 回调测试 use-after-free（整套并行跑时 SIGSEGV），并暴露 HTTP destroy 不抑制在途回调的不对称

**严重程度**: 🟡 Medium — 测试侧 UAF 导致 CI 整套并行运行时间歇 SIGSEGV（被 #033 的 fail-fast 长期掩盖）；并连带暴露一处产品契约不对称（待评估）

**状态**: Fixed（测试侧 + 产品侧 HTTP/SSE destroy 已与 WS 对齐）

**影响包**: `catcher-ffi`（测试）；观察项涉及 `catcher-http`（`ffi/http_ffi.rs`）

**位置**:
- `packages/catcher-ffi/tests/http_test.rs:301`（`make_result_cell`）、`:635`（`make_events_cell`）
- `packages/catcher-ffi/tests/quality_test.rs:18`（`make_callback_state`）
- `packages/catcher-http/src/ffi/http_ffi.rs:50`（全局 `runtime()`）、`:128`（`catcher_http_client_destroy`）、`:161/280/361/580`（`runtime().spawn` 回调任务）

---

## 现象

`cargo test -p catcher-ffi --test http_test` **单线程跑全过**，但**默认并行**整套跑时间歇性 **SIGSEGV**（PR #13 描述记录为"`h11_cancel_all_with_per_request` 附近崩溃，FFI 回调 + 全局 runtime 跨测试污染"）。由于 CI 未加 `--no-fail-fast`（见 [#033](./033-quality-test-flake-and-ci-fail-fast.md)），它长期被更靠前的 quality_test flake 掩盖，从未在 CI 暴露。

## 根因（测试侧 UAF）

FFI 用一个**全局 tokio runtime**（`http_ffi.rs:50`）执行请求，请求完成时在该 runtime 上**回调** `user_data`。`catcher_http_client_destroy`（`:128`）只做 `REGISTRY.remove(id)`，**不取消在途 spawn 任务、也不阻止其回调**（spawn 时已 clone 了 `Arc<HttpTransport>`，任务持有它直到自身结束）。

测试侧把回调目标这样传入：

```rust
let cell = Arc::new(Mutex::new(None::<String>));
let ptr = Arc::as_ptr(&cell) as *mut c_void;   // user_data 指向 Arc 内部数据
```

测试函数结束时 `cell`（Arc）被 drop → Mutex 释放。但若某个在途/被取消请求的回调在**测试返回后**才在全局 runtime 上触发（并行时多个测试时序交叠，极易发生），它会通过 `user_data` 解引用**已释放的内存** → use-after-free → SIGSEGV。单线程时各测试间的 `sleep` 恰好把回调排空，故不崩。

同样的模式存在于 `quality_test.rs` 的 `make_callback_state`（周期性探测回调 + abort 对同步回调有竞态窗口）。

## 修复（测试侧）

让 `user_data` 指向的内存**在进程生命周期内始终有效**——用 `Box::leak` 泄漏每个测试的回调状态，取代 `Arc::as_ptr`：

```rust
fn make_result_cell() -> (&'static Mutex<Option<String>>, *mut c_void) {
    let leaked: &'static Mutex<Option<String>> = Box::leak(Box::new(Mutex::new(None::<String>)));
    let ptr = leaked as *const Mutex<Option<String>> as *mut c_void;
    (leaked, ptr)
}
```

延迟回调因此始终写入有效内存，UAF 被彻底消除。每个测试泄漏一个小 Mutex，在测试进程内可忽略。`make_events_cell`、`quality_test::make_callback_state` 同样处理。

> 这同时也与正确的 FFI 契约一致：**宿主必须在回调可能触发的整个生命周期内保持 `user_data` 有效**。旧测试在 destroy 后立刻释放 `user_data`，本就违反该契约。

## 产品侧修复（已采用方案 A，与 WS 对齐）

此前 `catcher_http_client_destroy` / `catcher_sse_destroy` 仅 `REGISTRY.remove`，**在途请求/SSE 转发仍会在 destroy 后回调 `user_data`** —— 若宿主在 destroy 后释放 `user_data`，即产生与测试相同的 UAF。WS 在 [#15] 已通过 `cancelled_ws_ids` 标记 + 回调前检查解决；本次把同一机制对齐到 HTTP 与 SSE：

- **HTTP**（`http_ffi.rs`）：新增 `cancelled_http_ids` 集合；`destroy` 标记取消 + `cancel_all()` 取消在途请求 + `REGISTRY.remove`；所有异步回调点改用 `invoke_http_callback_if_active(id, …)`，destroy 后不再触发。
- **SSE**（`sse_ffi.rs`）：新增 `cancelled_sse_ids`；`destroy` 标记取消 + `close()` + remove；后台转发循环每轮检查取消并在回调点用 `invoke_sse_callback_if_active`，destroy 后停止转发。
- id 由 `HandleRegistry` 单调分配、不复用（`handle_registry.rs:43` `fetch_add`），故"已取消集合只增不清"安全。
- 仍存在与 WS 相同的极小 TOCTOU 窗口（检查通过后、回调前的瞬间被 destroy）；测试侧的 `Box::leak` 提供兜底，二者叠加后既无测试 UAF，又使 destroy 后回调的窗口收敛到与 WS 一致。

**遗留**：一次性 `catcher_sse_stream`（无句柄、不可取消）的回调会持续到流结束，宿主须在此期间保持 `user_data` 有效 —— 这是该 API 的固有契约，非回归。

## 影响范围小结

| 维度 | 评估 |
|------|------|
| 是否大改 | 否（测试侧：3 个 helper 改为 leak） |
| 是否产品 bug | 测试侧否；产品侧为待评估的契约不对称 |
| 价值 | 高 —— 消除 CI 整套并行运行的 SIGSEGV，配合 #033 恢复 CI 真实可信 |

## 验证

- `cargo test -p catcher-ffi --test http_test` 并行连跑 5 次：19 passed，无 SIGSEGV。
- `cargo test -p catcher-ffi --no-fail-fast`：codec(4) + http(19) + quality(5) + sse(3) 全过。
- `cargo clippy -p catcher-ffi --tests` clean。

## 关联

- [033-quality-test-flake-and-ci-fail-fast.md](./033-quality-test-flake-and-ci-fail-fast.md) — fail-fast 长期掩盖了本 SIGSEGV
- [002-ffi-callback-cstring-leak-risk.md](./002-ffi-callback-cstring-leak-risk.md) — FFI 回调内存历史问题
- PR #15 WS dispose UAF 修复（`cancelled_ws_ids`）—— HTTP destroy 可参照对齐
