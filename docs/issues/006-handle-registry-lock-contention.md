# Performance: FFI Handle 注册表 `Mutex<HashMap>` 锁竞争

**严重程度**: 🟡 Low — 当前 FFI 调用量不大，但在高并发场景下会成为瓶颈

**状态**: Open

**位置**:

| 注册表 | 文件 | 场景 |
|--------|------|------|
| `HANDLES: Mutex<Option<HashMap<usize, Arc<HttpTransport>>>>` | `packages/catcher-http/src/ffi/http_ffi.rs:12` | 每 HTTP FFI 调用读取 |
| `WS_HANDLES: Mutex<Option<HashMap<usize, Arc<WsHandle>>>>` | `packages/catcher-ws/src/ffi/ws_ffi.rs:12` | 每 WS FFI 调用读取 |
| `SSE_HANDLES: Mutex<Option<HashMap<usize, Arc<TokioMutex<SseClient>>>>>` | `packages/catcher-http/src/ffi/sse_ffi.rs:20` | 每 SSE FFI 调用读取 |

---

## 模式

```rust
static HANDLES: std::sync::Mutex<Option<HashMap<usize, Arc<HttpTransport>>>> = std::sync::Mutex::new(None);

fn handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<HttpTransport>>>> {
    HANDLES.lock().unwrap()
}

// 使用
let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
```

## 问题

1. **读写不区分**：绝大多数 FFI 调用是读取（`.get(&id)`），但 `Mutex` 强制所有读写串行化
2. **批量注册表**：三个 FFI 模块各自维护一份结构相同但类型不同的注册表，代码重复
3. **锁持有域**：`.cloned()` 在锁内执行（clone 一个 `Arc`），虽然很快但严格来说不需锁保护

## 建议

### 短期：`RwLock` 替代 `Mutex`

```rust
static HANDLES: std::sync::RwLock<HashMap<usize, Arc<HttpTransport>>> = std::sync::RwLock::new(HashMap::new());

// 读取（无竞争）
let transport = HANDLES.read().unwrap().get(&id).cloned();

// 写入（插入/删除）
HANDLES.write().unwrap().insert(id, Arc::new(transport));
```

### 长期：泛型 `HandleRegistry<T>`

将三个注册表合并为一个泛型实现，归入 `catcher-core`：

```rust
// catcher-core
pub struct HandleRegistry<T: Send + Sync> {
    map: RwLock<HashMap<usize, Arc<T>>>,
    next_id: AtomicUsize,
}
```

## 关联

- 重复代码：RUST_STYLE_GUIDE.md 附录 B（P2 优先级）
