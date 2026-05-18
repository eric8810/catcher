# 00 — 网络场景扩展调研汇总报告

> 调研日期：2025-07-18
> 覆盖 6 个维度，42+ 个细分场景领域
> 各阶段详细报告：`01-protocols.md` ~ `06-security.md`

---

## 一、场景覆盖矩阵

### 总览

| 维度 | 已覆盖 | 部分覆盖 | 未覆盖 | 关键发现数 |
|------|:------:|:------:|:------:|:--------:|
| A. 网络通讯协议 | 12 | 8 | 10 | 10 |
| B. 网络环境与拓扑 | 8 | 5 | 14 | 8 |
| C. 硬件与设备 | 4 | 4 | 6 | 4 |
| D. 软件运行环境 | 3 | 5 | 7 | 5 |
| E. 用户交互 | 5 | 6 | 9 | 7 |
| F. 安全与攻击 | 12 | 5 | 8 | 7 |
| **合计** | **44** | **33** | **54** | **41** |

---

## 二、按严重度分级的关键发现

### 🔴 高优先级（应立即产生测试用例或代码变更）：共 25 项

#### 协议层面（来自阶段一）

1. **HTTP 408 Request Timeout 应重试** — 当前所有 4xx 为 NonRetryable，但 408 是 keepalive race 信号。需修改 `ErrorCategory::category()` 将 408 归为 Retryable。
2. **HTTP 429 Retry-After 支持** — retry 策略需读取 `Retry-After` header 并延迟重试。
3. **WS 连接断开时 send() 不应无限排队** — 参考 tokio-tungstenite Issue #35，需增加 backpressure 或 max_pending_frames。
4. **SSE 跨 chunk UTF-8 码点** — 3 字节字符被切在 chunk 边界时，需使用字节级 buffering 而非 `String::from_utf8_lossy`。
5. **HTTP/2 GOAWAY 后重试** — 确保 GOAWAY 前已发送但未收到响应的请求被正确重试。
6. **SSE BOM (U+FEFF) 处理** — 流开头 BOM 应被静默过滤。
7. **重定向时 Authorization header 剥离** — 验证 reqwest 在跨域重定向时移除敏感 header。
8. **多端点竞速 DNS 去重** — 多个端点解析到同一 IP 时应检测并去重。

#### 网络环境（来自阶段二）

9. **CGNAT 空闲超时** — keepAlive interval 默认 30s，需文档说明应配置为 < 60s 以兼容 CGNAT。
10. **IPv6 host_mapping** — `host_mapping` 需支持 IPv6 地址映射。
11. **LB idle timeout < client keepAlive** — 文档警告：需对齐客户端和服务端空闲超时。
12. **代理返回非预期 Content-Type** — 代理返回 HTML 而非 JSON 时的错误信息需清晰。
13. **网络闪断时 CB 误触发** — 短断连 (100-500ms) 不应触发 circuit breaker。

#### 硬件与设备（来自阶段三）

14. **ARM64 Linux/aarch64-unknown-linux-gnu CI** — musl build 未在 CI 中覆盖。
15. **移动 OS 后台 SSE/WS 连接** — 需文档说明 iOS/Android 后台限制。

#### 软件环境（来自阶段四）

16. **Alpine/musl DNS 差异** — DNS 超时和 `search domains` 行为与 glibc 不同。
17. **WKWebView cookie 阻止** — `credentials: 'include'` 在 iOS WKWebView 可能被静默忽略。

#### 用户交互（来自阶段五）

18. **双重 destroy 幂等性** — destroy 调用两次不应 panic 或 double-free。
19. **429 限流风暴** — 所有请求同时收到 429 时不应同时重试（需全局抑制）。
20. **重连与手动 close 竞态** — close 优先级必须高于 reconnect。
21. **null 回调/参数在 FFI 边界** — 验证 `EventCallback` 为 null 时不调用。

#### 安全（来自阶段六）

22. **CRLF 注入检查** — header value 和 URL 参数中不应允许 `\r` `\n`。
23. **msgpack `max_unpack_size`** — 防止恶意超大数据包 OOM。
24. **msgpack 嵌套深度限制** — 防止栈溢出（深度 > 100 层）。
25. **并发 FFI 调用线程安全** — 验证多线程同时调用 C ABI 函数的安全性。

---

### 🟡 中优先级（应纳入设计文档或 roadmap）：共 16 项

26. HTTP/3 (QUIC) 路线图调研
27. WS perMessageDeflate 长连接内存监控
28. SSE 代理缓冲文档警告（企业网络兼容性）
29. PAC/WPAD 代理自动发现 — 文档说明不支持
30. NTLM 代理认证 — 文档说明不支持
31. Gray failure 下的 retry 策略敏感度
32. VPN 网络变化检测（连接池是否重建）
33. 容器 DNS 并发限制 (127.0.0.11)
34. macOS App Sandbox 网络 entitlement 文档
35. 24h 长时间运行内存泄漏 test harness
36. 连接建立时 DNS/TLS 极端慢的超时覆盖
37. JSON 解析深度限制
38. DNS rebinding 防护文档（beforeRedirect 过滤内网 IP）
39. 快速创建/销毁循环下的资源泄漏检测
40. callback 单次触发保证（use-after-free 安全）
41. TLS 指纹差异（rustls vs native-tls）文档

---

## 三、推荐测试用例增量清单

基于以上发现，按 catcher 测试层级分类：

### 3.1 已有测试设计需补充的用例

在现有 `arch-ts/11-http-tests.md`、`arch-ts/13-api-gap-tests.md` 基础上增加：

| 编号 | 类别 | 用例 | 严重度 |
|:----:|------|------|:------:|
| R14 | Retry | 408 Request Timeout 应重试 | 🔴 |
| R15 | Retry | 429 读取 Retry-After header 延迟 | 🔴 |
| R16 | Retry | 429 风暴全局抑制（同一 host 收到 429 后暂停所有请求） | 🔴 |
| H20 | HTTP | 响应 Content-Type 非预期时不 panic | 🔴 |
| H21 | HTTP | CRLF 注入 header value 被拒绝 | 🔴 |
| H22 | HTTP | URL 中 CRLF 被过滤 | 🔴 |
| C10 | CORS | WKWebView credentials include 文档说明 | 🟡 |
| P10 | Proxy | 代理返回 HTML 时错误信息可读 | 🔴 |
| P11 | Proxy | 代理认证失败 407 正确分类 | 🔴 |
| ST7 | Stream | 大文件下载中途 cancel 后资源释放 | 🔴 |
| DNS5 | DNS | IPv6 host_mapping 映射 | 🔴 |
| DNS6 | DNS | 多 A 记录时第一个 IP 失败尝试其余 | 🟡 |
| RD9 | Redirect | 跨域重定向 Authorization 剥离验证 | 🔴 |

### 3.2 新增测试类别

#### 长时运行稳定性测试（新增文件：`test/stability/long-running.test.ts`）

| 编号 | 用例 | 严重度 |
|:----:|------|:------:|
| LR1 | 1000 次 create/destroy 循环 — fd/resident memory 不增长 | 🔴 |
| LR2 | 24h WS 连接 — 内存/cpu/fd 稳定 | 🟡 |
| LR3 | 24h SSE 连接+自动重连 — 无连接泄漏 | 🟡 |
| LR4 | 双重 destroy — 不 panic | 🔴 |

#### FFI 边界安全测试（新增文件：`crates/catcher-ffi/tests/`）

| 编号 | 用例 | 严重度 |
|:----:|------|:------:|
| FFI1 | `catcher_http_execute` — null handle 返回错误 | 🔴 |
| FFI2 | `catcher_ws_send_text` — null message 返回错误 | 🔴 |
| FFI3 | EventCallback 为 null — 不调用 | 🔴 |
| FFI4 | 多线程并发调用 — 不 data race | 🔴 |
| FFI5 | callback 单次触发 — 不 use-after-free | 🟡 |

#### msgpack 安全测试（新增用例在 `src/codec/msgpack.rs`）

| 编号 | 用例 | 严重度 |
|:----:|------|:------:|
| MP1 | unpack 超大数据包 — max_unpack_size 限制 | 🔴 |
| MP2 | unpack 深度嵌套 (>100 层) — 拒绝或截断 | 🔴 |
| MP3 | unpack ext 类型 — 正确处理 | 🟡 |

#### 网络拓扑模拟测试（利用现有 NetworkProxy 扩展）

| 编号 | 用例 | 严重度 |
|:----:|------|:------:|
| NET1 | CGNAT 模拟 — 60s 空闲后断开 | 🔴 |
| NET2 | 网络闪断 — 300ms 断连不触发 CB | 🔴 |
| NET3 | IPv6-only → Happy Eyeballs → IPv4 回退 | 🟡 |
| NET4 | 多端点 DNS 去重 — 同一 IP 不重复竞速 | 🔴 |

---

## 四、需要代码变更的关键项

| 变更 | 影响文件 | 严重度 |
|------|---------|:------:|
| `ErrorCategory::category()` 将 HTTP 408 归为 Retryable | `catcher-core/src/error.rs` | 🔴 |
| RetryConfig 增加 `respect_retry_after: bool` 字段 | `catcher-core/src/types/resilience.rs` | 🔴 |
| WS send 增加 `max_pending_frames` 限制 | `catcher-ws/src/transport/ws_client.rs` | 🔴 |
| SSE chunk buffer 改用字节级 buffering (处理跨 chunk UTF-8) | `catcher-http/src/sse/` | 🔴 |
| msgpack unpack 增加 `max_size` 和 `max_depth` 参数 | `catcher-core/src/codec/` | 🔴 |
| host_mapping 支持 IPv6 地址 | `catcher-http/src/transport/dns.rs` | 🔴 |
| Header value CRLF 过滤 | `catcher-http/src/transport/http_client.rs` | 🔴 |
| CB 增加 `min_failure_window_ms` 防止闪断误触发 | `catcher-http/src/resilience/circuit_breaker.rs` | 🟡 |

---

## 五、调研文件索引

| 文件 | 内容 |
|------|------|
| `docs/plan/05-expansion-research.md` | 调研计划 |
| `docs/research/expandation/01-protocols.md` | 阶段一：协议场景 |
| `docs/research/expandation/02-network-env.md` | 阶段二：网络环境 |
| `docs/research/expandation/03-hardware.md` | 阶段三：硬件设备 |
| `docs/research/expandation/04-software-env.md` | 阶段四：软件环境 |
| `docs/research/expandation/05-user-interaction.md` | 阶段五：用户交互 |
| `docs/research/expandation/06-security.md` | 阶段六：安全场景 |
| `docs/research/expandation/00-summary.md` | 本汇总报告 |

---

## 六、结论

本次调研通过 6 个维度、42+ 个细分领域，系统性地识别了 catcher 当前设计中未覆盖的** 25 个高优先级**和 **16 个中优先级**场景。

最紧迫的行动项：

1. **修改 HTTP 408 的重试分类** — 1 行代码变更，解决 keepalive race 的真实生产问题
2. **增加 msgpack 输入大小和深度限制** — 防止 DoS 攻击
3. **header value CRLF 过滤** — 防止 HTTP 注入
4. **FFI 边界 null 安全** — 已有 ISSUE #14 的教训，需系统性加固
5. **CGNAT/NAT 空闲超时文档** — 帮助用户正确配置 keepAlive interval

建议在下一轮迭代中优先修复 `🔴 高优先级` 的代码变更项，其余纳入测试 backlog。
