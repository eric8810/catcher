# Phase F — 协议合规矩阵（RFC 逐条对照）

> 状态: 5/16 已完成，11/16 待调研

---

## HTTP/1.1 (RFC 9110)

| # | 要求 | RFC | Catcher | 状态 |
|---|------|-----|:------:|:----:|
| H1 | 408 MAY retry on new connection | §15.5.7 | ✅ 已修复 | ✅ |
| H2 | 425 MUST retry without early data | §15.5.8 | ❌ 未实现 | ⬜ |
| H3 | 429 SHOULD honor Retry-After | RFC 6585 | ❌ RetryAfter 未解析 | ⬜ |
| H4 | 301/302 cross-origin SHOULD strip Authorization | §15.4 | ❌ 未实现 | ⬜ |

## HTTP/2 (RFC 9113 / 7540)

| # | 要求 | RFC | Catcher | 状态 |
|---|------|-----|:------:|:----:|
| H5 | GOAWAY: retry streams > last_stream_id | 7540 §6.8 | ❌ 依赖 reqwest/hyper, 未感知 | ⬜ |
| H6 | SETTINGS_MAX_CONCURRENT_STREAMS: respect limit | 7540 §6.5.2 | ❌ 无流计数 | ⬜ |
| H7 | 421 Misdirected Request: retry on different connection | 7540 §9.1.2 | ❌ 未实现 | ⬜ |
| H8 | HPACK dynamic table: per-connection state | 7541 §2.3 | ✅ 由 HTTP 库处理 | ✅ |

## WebSocket (RFC 6455)

| # | 要求 | RFC | Catcher | 状态 |
|---|------|-----|:------:|:----:|
| W1 | Close 1006 (abnormal): SHOULD reconnect | §7.2 | ✅ | ✅ |
| W2 | Close 1000/1001: SHOULD NOT reconnect | §7.2 | ✅ | ✅ |
| W3 | Ping without Pong: SHOULD treat as connection failure | §5.5.2 | ⚠️ 需验证 | ⬜ |
| W4 | perMessageDeflate: SHOULD limit memory | RFC 7692 | ⚠️ 未限制 zlib 上下文 | ⬜ |

## SSE (WHATWG HTML §9.2)

| # | 要求 | WHATWG | Catcher | 状态 |
|---|------|--------|:------:|:----:|
| S1 | Network error: MUST reconnect | §9.2 | ✅ | ✅ |
| S2 | BOM (U+FEFF): SHOULD silently filter | §9.2 | ❌ 全仓零命中 | ⬜ |
| S3 | Last-Event-ID: MUST send on reconnect | §9.2 | ✅ | ✅ |
| S4 | MIME type != text/event-stream: treat as network error | §9.2 | ⚠️ 需验证 | ⬜ |
