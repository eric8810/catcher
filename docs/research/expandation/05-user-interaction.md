# 阶段五：用户交互与极端场景调研

> 调研日期：2025-07-18
> 范围：应用生命周期、并发模式、用户行为、资源约束

---

## E1. 应用生命周期

### E1.1 客户端创建与销毁

| 场景 | 描述 | catcher 覆盖 | 建议测试 |
|------|------|:----------:|---------|
| 快速创建/销毁 | 无请求情况下 create→destroy | ❌ | 验证无 panic / 无资源泄漏 |
| 销毁时飞行请求 | destroy 时仍有请求进行中 | ⚠️ cancelAll | 验证 cancelAll+destroy 的顺序正确性 |
| 双重销毁 | destroy 调用两次 | ❌ | 验证不会 double-free |
| Finalizer 触发时机 | GC 回收时自动 destroy | ⚠️ Dart Finalizer | 验证 Finalizer 晚于 cancelAll |
| 全局单例 vs 多实例 | 多个 HttpClient 指向同一 host | ✅ SharedAgent | 验证连接池统计不混算 |

### E1.2 配置热更新

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| `updateConfig()` 修改 retry | ✅ G11 | 验证进行中请求不受影响 |
| 运行时修改 CB threshold | ❌ | 不支持但不应 panic |
| 运行时修改 proxy URL | ❌ | 影响后续请求 |
| 运行时修改 DNS host_mapping | ❌ | 需清除 DNS cache |

---

## E2. 极端并发与资源耗尽

### E2.1 高并发场景

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| 1000+ 并发请求 | Semaphore + 连接池压力 | ⚠️ | 验证无 panic，拒绝而非崩溃 |
| 单 host 连接耗尽 | pool_max_idle 到达上限 | ✅ | 验证等待队列超时 |
| 速率限制 (429) 风暴 | 所有请求同时收到 429 | ❌ | 应全局暂停而非逐个重试（thundering herd） |
| 内存压力 | 大量大 body 同时加载 | ⚠️ execute_stream | 验证 stream 模式内存释放及时 |

### E2.2 资源泄漏压力测试

| 场景 | 建议 |
|------|------|
| 24h 长时间运行 — 1000 创建/销毁循环 | 验证 fd/resident memory 不增长 |
| 24h 长时间运行 — 长时间 WS 连接 | 验证无内存泄漏（事件监听器/timer/zlib context） |
| 24h 长时间运行 — SSE 长连接+自动重连 | 验证重连不泄漏连接 | 

---

## E3. 弱网与极端网络条件

catcher 的 E2E 测试已覆盖 `good`/`weak`/`veryWeak`/`metro`/`highLatency`/`packetLoss`/`congested` 这 7 种预设。缺失的真实场景：

### E3.1 速率极端情况

| 场景 | 带宽 | 建议 |
|------|------|------|
| 极低带宽 (2G) | 10-50 kbps | 验证超时设置是否合理 |
| 带宽波动 | 0→10Mbps→0 循环 | adaptive timeout 的稳定性 |
| 零带宽检测 | TCP 阻塞但不报错 | 依赖 OS TCP keepalive (默认 2h) |

### E3.2 连接建立极端情况

| 场景 | 建议 |
|------|------|
| DNS 解析极慢（5s+） | 验证 connect_timeout 是否覆盖 DNS |
| TCP SYN 被丢弃（无 ICMP） | 验证 connect_timeout 到期后正确报错 |
| TLS 握手极慢（大证书链 CRL/OCSP） | 验证 TLS 握手超时是否覆盖 |

### E3.3 用户行为

| 场景 | 建议 |
|------|------|
| 快速页面切换（SPA）— 创建→1请求→销毁 | 验证短生命周期无泄漏 |
| 用户快速点击重试按钮 | 验证同一请求的去重 |
| 浏览器 Tab 后台化 | 验证 `requestAnimationFrame` 相关逻辑暂停 |
| 浏览器关闭时 SSE 连接 | 验证 `beforeunload` 中 close() |

---

## E4. 错误与恢复场景

### E4.1 错误处理边界

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| `catcher_http_execute` 传入 null handle | ✅ FFI 规则 null check | 验证返回错误而非 segfault |
| `catcher_ws_send_text` 传入 null message | ⚠️ | 验证 `FfiString` 为 null 时行为 |
| 回调函数为 null | ❌ | 验证不调用回调，不 crash |
| 配置 JSON 为非法 UTF-8 | ⚠️ | 验证 `CString::new` 前的 null byte 过滤 |

### E4.2 罕见竞态

| 场景 | 描述 | 建议 |
|------|------|------|
| connect 超时同时 cancel | cancel 和 connect timeout 竞态 | 验证最终状态一致 |
| send 与 close 竞态 | close 在 send 完成前执行 | 验证无 send-after-close 错误 |
| 重连与手动 close 竞态 | 重连 timer 触发同时 close | 验证 close 优先级高于 reconnect |
| CB half-open 与 cancelAll | half-open 期间 cancelAll | 验证 cancel 后不误报 CB 失败 |

---

## 阶段五总结：关键缺失

1. **双重 destroy** — 验证幂等性
2. **429 风暴 + thundering herd** — 全局时延策略
3. **24h 长时间运行内存泄漏** — connection/socket/zlib/timer 泄漏
4. **快速创建/销毁循环** — 资源泄漏压力测试
5. **连接建立时 DNS/TLS 极端慢** — 超时覆盖范围验证
6. **重连与手动 close 竞态** — close 必须优先
7. **null 回调/参数** — FFI 边界健壮性

---

## 引用来源

1. AWS Architecture Blog, "Exponential Backoff And Jitter" (thundering herd mitigation), https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
2. tokio-tungstenite Issue #35, "WebSocketStream Sink implementation doesn't apply back-pressure" (send queue unbounded), https://github.com/snapview/tokio-tungstenite/issues/35
3. OneUptime, "How to Fix 'Memory Leak' Issues in WebSocket Servers" (event listener accumulation, timer leak), https://oneuptime.com/blog/post/2026-01-24-websocket-memory-leak-issues/view
4. "The WebSocket Connection Leak That Cost Us $40K in AWS Bills," https://javascript.plainenglish.io/the-websocket-connection-leak-that-cost-us-40k-in-aws-bills-0871a61acdaa
