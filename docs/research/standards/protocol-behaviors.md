# 协议行为标准 — TCP/TLS/DNS/HTTP/WS/SSE/QUIC 客户端行为规范

> 每个 RFC 中 "客户端应该做什么" 与 Catcher 实际行为的对照。标注 🟢 已实现 / 🟡 部分 / 🔴 未实现 / ⚪ 不适用。

---

## 一、TCP (Transmission Control Protocol)

### 1.1 RFC 6298 — 重传超时 (RTO)

| 要求 | 内容 | Catcher |
|------|------|:------:|
| 初始 RTO | **1 秒** (SHOULD)，之前是 3s | ⚪ (内核处理) |
| RTO 最小值 | **1 秒** (SHOULD，保守) | ⚪ |
| RTO 最大值 | **≥ 60 秒** (MAY) | ⚪ |
| RTO 计算公式 | SRTT + max(G, 4 × RTTVAR)，alpha=1/8, beta=1/4 | ⚪ |
| Karn 算法 | 不用重传段采样 RTT | ⚪ |

**对 Catcher 的影响**：TCP RTO 最小值 1s 意味着——如果应用层 timeout < 1s，可能在 TCP 重传之前就放弃了。Catcher 的 `response_timeout` 应始终 ≥ 2s。

### 1.2 RFC 5482 — TCP 用户超时

TCP 用户超时是 "连接多久收不到 ACK 后强制关闭"。典型 Linux 默认值：

```
tcp_retries2 = 15 → 约 13-30 分钟（动态计算）
```

这意味着——如果发生路由黑洞且没有应用层超时，TCP **会在数十分钟后才关闭连接**。Catcher 的应用层超时（默认 30s）是必须的。

### 1.3 RFC 8312 — CUBIC 拥塞控制（Linux 默认）

| 特征 | 说明 |
|------|------|
| 窗口增长 | 三次函数，与 RTT 无关（对长肥管道友好） |
| 丢包检测 | 检测到丢包后，窗口乘性减小 (β=0.7 for fast convergence, 0.3ms for regular) |
| 对 Catcher 的影响 | CUBIC 在丢包后恢复速度快于传统 Reno |

### 1.4 RFC 1122 §4.2.3.6 — TCP KeepAlive

| 参数 | Linux 默认 | Windows 默认 | macOS 默认 |
|------|:--------:|:----------:|:--------:|
| 空闲时间 | **7200s (2h)** | 7200s | 7200s |
| 探测间隔 | **75s** | 1s | 75s |
| 探测次数 | **9** | 5-10 | 8 |
| 总计超时 | 2h + 9×75s ≈ 2h11m | 2h + 10×1s ≈ 2h | ~2h10m |

**重要**：TCP KeepAlive 默认 2 小时才发第一个探测包。对于 HTTP keepAlive（应用层），这个机制**完全不可依赖**。Catcher 的 keepAlive interval 必须是**秒级**的（如 30s）。

### 1.5 RFC 3481 — 2.5G/3G 无线网络上的 TCP

| 建议 | 说明 |
|------|------|
| 更大的初始窗口 | 无线链路初始 BW 可能较低，但延迟高 |
| 避免激进重传 | 无线链路的丢包不一定是拥塞，可能是信号衰落 |
| 支持 SACK | 减少不必要的重传 |

---

## 二、TLS (Transport Layer Security)

### 2.1 RFC 8446 — TLS 1.3

| 特性 | 参数 | Catcher |
|------|------|:------:|
| 1-RTT 握手 | ClientHello → ServerHello → Finished: **1 个 RTT** | 🟢 (rustls 支持) |
| 0-RTT 重续 | PSK + early_data: **0 个 RTT**（有重放风险） | 🟡 (支持但需评估) |
| 0-RTT 重放攻击 | 服务端返回 425 Too Early → 客户端**必须重试** | 🔴 (425 未特殊处理) |
| Session Ticket | 服务端签发 + 客户端存储 | 🟢 |
| OCSP Stapling | 证书状态内嵌在握手中 | 🟢 |
| Certificate Transparency | 非 TLS 1.3 核心要求 | ⚪ |

### 2.2 典型握手延迟

| 场景 | 额外延迟 | 说明 |
|------|:------:|------|
| TLS 1.2 完整握手 | 2 RTT (4 趟) | 通常 200-400ms |
| TLS 1.3 完整握手 | 1 RTT (2 趟) | 通常 100-200ms |
| TLS 1.3 0-RTT | 0 RTT | 仅限已访问过的服务 |
| 证书链下载 (中间 CA) | +1-2 RTT | 如果中间证书不在客户端缓存 |
| OCSP 查询 | +0.5-2s | 如果未 Stapling |
| CRL 下载 (极少) | +1-10s | 大型 CRL 文件 |

### 2.3 TLS 错误分类

| 错误 | Catcher 分类 | 正确？ |
|------|:---------:|:-----:|
| 证书过期 | NonRetryable | 🟢 |
| 域名不匹配 | NonRetryable | 🟢 |
| 自签名证书 | NonRetryable (除非 reject_unauthorized=false) | 🟢 |
| 证书链不完整 | NonRetryable | 🟢 |
| TLS 握手超时 | 🔴 当前行为不明 | 应为 Retryable (网络问题) |
| TLS 版本不匹配 | NonRetryable | 🟢 |

---

## 三、DNS (Domain Name System)

### 3.1 RFC 1034/1035 — DNS 解析

| 行为 | 标准/典型值 | Catcher |
|------|-----------|:------:|
| 解析超时 | 无标准默认，典型 5s | 🟡 (依赖 hickory-resolver) |
| 重试次数 | 典型 2-3 次 | 🟡 |
| TTL 语义 | ≤ TTL 秒内可使用缓存 | 🟢 |
| TTL=0 | 不应缓存 | 🟡 |
| serve-stale (RFC 8767) | TTL 过期后仍可用旧记录（异步刷新） | 🔴 未实现 |

### 3.2 RFC 8305 — Happy Eyeballs v2

| 要求 | 参数 | Catcher |
|------|------|:------:|
| 同时发起 A 和 AAAA 查询 | — | 🟡 (hickory-resolver) |
| IPv6 连接先尝试，**250ms** 后启动 IPv4 | Resolution Delay = 250ms | 🔴 (未验证) |
| 连接竞速：同时尝试多个地址 | Connection Attempt Delay = 250ms | 🔴 (未验证) |
| IPv6 快速回退 | 如果 IPv6 连接失败（非超时），立即尝试 IPv4 | 🔴 |

### 3.3 DNS 错误分类

| 错误 | 含义 | Retryable? |
|------|------|:--------:|
| NXDOMAIN | 域名不存在 | **NonRetryable** |
| SERVFAIL | DNS 服务器出错 | **Retryable** (可能是临时故障) |
| 超时 | 无响应 | **Retryable** (网络问题) |

---

## 四、HTTP/1.1 (RFC 9110, 取代 7230-7235)

### 4.1 关键状态码的客户端行为要求

| 状态码 | RFC 9110 § | 定义 | 客户端行为 | Catcher |
|--------|:--------:|------|-----------|:------:|
| **408 Request Timeout** | 15.5.7 | 服务端在超时前未收到完整请求 | **"客户端 MAY 在新连接上重试"** | 🔴 **当前归类为 NonRetryable** |
| **425 Too Early** | 15.5.8 | 0-RTT 请求被拒绝 | **"自动重试"** | 🔴 |
| **429 Too Many Requests** | — (RFC 6585) | 速率限制 | **读取 Retry-After header** | 🔴 |
| **502 Bad Gateway** | 15.6.3 | 上游服务器返回无效响应 | **可重试** | 🟢 |
| **503 Service Unavailable** | 15.6.4 | 服务暂时不可用 | **可重试，注意 Retry-After** | 🟢 |
| **504 Gateway Timeout** | 15.6.5 | 上游超时 | **可重试** | 🟢 |

### 4.2 HTTP KeepAlive

| 参数 | RFC 建议 | 典型实现 |
|------|---------|---------|
| 空闲超时 | 无标准默认 | 服务端: nginx 75s / Apache 5s |
| 最大请求数 | 无限制 | nginx: `keepalive_requests 1000` |
| 客户端行为 | 应复用空闲连接 | 🟢 Catcher 已有 |

### 4.3 重定向行为

| 状态码 | RFC 9110 | 方法变更？ | Authorization 剥离？ |
|:-----:|---------|:--------:|:------------------:|
| 301 | 永久重定向 | GET→GET, POST→**可能变 GET** | 🔴 跨域时应剥离 |
| 302 | 临时重定向 | 通常变为 GET | 🔴 跨域时应剥离 |
| 307 | 临时重定向 | **方法不变** | 🔴 跨域时应剥离 |
| 308 | 永久重定向 | **方法不变** | 🔴 跨域时应剥离 |

**对 Catcher 的建议**：`beforeRedirect` 钩子中应检查跨域并剥离 Authorization + Cookie headers。

---

## 五、HTTP/2 (RFC 7540 / RFC 9113)

### 5.1 核心参数

| 参数 | 默认值 | 说明 |
|------|:-----:|------|
| SETTINGS_MAX_CONCURRENT_STREAMS | 无限制 (0 = 不限制) | 单连接并发流数 |
| SETTINGS_INITIAL_WINDOW_SIZE | **65,535 字节** | 流控初始窗口 |
| HPACK 动态表大小 | **4,096 字节** | 请求头压缩 |

### 5.2 GOAWAY 帧处理 ⚠️ Catcher 关键缺失

```
服务端 → 客户端：GOAWAY (last_stream_id=100)
含义：流 100 及之前的已处理，101+ 的需要在新连接上重试

客户端行为 (RFC 7540 §6.8):
  1. 停止在旧连接上发送新请求
  2. 将 >last_stream_id 的未完成请求在新连接上重试
  3. 等待正在进行的流完成
  4. 发送 PING 确认连接存活
```

**Catcher 当前状态**：依赖 reqwest/hyper 处理 GOAWAY，但 retry/CB 层不感知。

### 5.3 HTTP/2 KeepAlive 差异

| 对比 HTTP/1.1 | HTTP/2 |
|:---|------|
| 连接复用 | 1 个连接 = N 个并发流 |
| 空闲检测 | **PING 帧**（不是 TCP keepalive） |
| max_idle_per_host | **无意义**（只需 1 个连接） |

---

## 六、HTTP/3 + QUIC (RFC 9000/9001/9002/9114)

### 6.1 QUIC 核心特性

| 特性 | 参数 | 对 Catcher 的影响 |
|------|------|-----------------|
| 0-RTT 握手 | 支持 | 类似 TLS 1.3 0-RTT |
| **连接迁移** | **IP 地址变化不中断连接！** | 🔴 **Catcher 核心价值场景—WiFi↔Cellular 切换** |
| 空闲超时 | 默认 30s (max_idle_timeout) | 超过 30s 无数据则连接关闭 |
| PING 帧 | 用于 keepalive | 类似 HTTP/2 PING |
| 丢包恢复 | 更快的 RTT 估计 + 更积极的探测 | 比 TCP 更快检测丢包 |
| 拥塞控制 | 类似 TCP NewReno (可插拔) | RFC 9002 附录 B |

### 6.2 QUIC 连接迁移详解

```
场景：用户在手机上从 WiFi 切换到 4G
  TCP:  IP 变 → 所有连接 RST → 全部重连 → 最长 5s 才能恢复
  QUIC: IP 变 → 发送 PATH_CHALLENGE → 新路径验证 → 连接继续，0 中断！
```

**对 Catcher**：一旦支持 QUIC/HTTP3，WiFi↔Cellular 切换这种场景 Catcher 几乎不需要做什么。

---

## 七、WebSocket (RFC 6455)

### 7.1 连接生命周期

| 事件 | 规范要求 | Catcher |
|------|---------|:------:|
| 握手 401/403 | **不应重连**（认证失败） | 🟢 |
| 握手 3xx | **SHOULD NOT 跟随重定向**（RFC 6455 §4.1） | 🟡 需要验证 |
| Close 帧 | 双向关闭握手 (2 帧) | 🟢 |
| **Ping 无 Pong** | **应视为连接失败** | 🔴 需要验证超时策略 |
| 帧大小超限 | **连接应关闭 (1009 Message Too Big)** | 🟢 |
| 分片消息 | 控制帧可穿插，但分片之间不能穿插其他数据帧 | 🟢 |

### 7.2 Close 码

| 码 | 含义 | Catcher 行为 |
|:--:|------|:---------:|
| 1000 | 正常关闭 | 不重连 |
| 1001 | 端点离开 (如页面关闭) | 不重连 |
| 1006 | 异常关闭 (无 close 帧) | **应重连** |
| 1009 | 消息过大 | 不重连 |
| 1011 | 服务器错误 | 可重连 |

### 7.3 RFC 7692 — perMessageDeflate

| 参数 | 说明 |
|------|------|
| `client_max_window_bits` | 压缩窗口，默认 15 |
| `server_max_window_bits` | 同上 |
| 内存风险 | zlib 上下文可能持续增长（ws#1617 报告的泄漏） |

---

## 八、SSE (WHATWG HTML Standard §9.2)

### 8.1 重连行为

| 场景 | WHATWG 规定 | Catcher |
|------|-----------|:------:|
| 网络错误 | **必须重连** | 🟢 |
| HTTP 204 No Content | **必须不重连** | 🟢 |
| HTTP 305 Use Proxy | **必须不重连** | 🟢 |
| HTTP 200 OK + text/event-stream | 开始处理流 | 🟢 |
| 其他 HTTP 响应 | **应重连**（包括 5xx） | 🟡 |
| MIME 类型不是 text/event-stream | **应视为网络错误并重连** | 🟡 |

### 8.2 重连参数

| 参数 | WHATWG 默认 | Catcher |
|------|:---------:|:------:|
| 重连时间 (retry field) | **实现定义**（Chrome: ~3s, Firefox: ~5s） | 🟢 |
| `retry:` 为 0 | 浏览器行为是 [implementation-defined](https://infra.spec.whatwg.org/#implementation-defined) | 🟡 |
| `Last-Event-ID` | 重连时通过 HTTP header 发送 | 🟢 |
| BOM (U+FEFF) | 流开头应**静默过滤** | 🔴 全仓零命中 |

### 8.3 字段解析

```
流格式：
  : 注释行（忽略）
  event: eventName\n    ← 事件类型，默认 "message"
  data: line1\ndata: line2\n  ← 数据，多行拼接（用 \n 分隔）
  id: 12345\n           ← 事件 ID（Last-Event-ID 的值）
  retry: 3000\n         ← 重连间隔 ms
  \n                    ← 空行分隔事件
```

---

## 九、QUIC 丢包检测 (RFC 9002) ⚠️ 对 Catcher 超时策略的重要参考

### 9.1 关键参数

| 参数 | 默认值 | TCP 等价 |
|------|:-----:|---------|
| 初始 RTT 估计 | 333ms (kInitialRtt) | 1s (RFC 6298) |
| 探测超时 (PTO) | SRTT + max(4×RTTVAR, kGranularity) + ack_delay | RTO |
| 最小探测超时 | **kMinPacketNumber* = ??? | 1s (TCP RTO min) |
| 丢包检测 | **packet_threshold=3** (3 个后续包被 ACK → 判定丢失) | 3 dup ACK |
| 时间阈值 | **9/8 × max(SRTT, latest_rtt)** | RTO |

### 9.2 对 Catcher 的影响

QUIC 的丢包检测**远超 TCP 的速度**：TCP 需要 RTO (min 1s) 或 3 dup ACK，QUIC 只需要 3 个后续包被确认（几毫秒到几十毫秒）。这意味着：
- 基于 QUIC/HTTP3 的应用层 retry 延迟会大幅降低
- Catcher 的 `min_backoff` 在 QUIC 场景下可以显著缩短
