# 阶段六：安全与攻击场景调研

> 调研日期：2025-07-18
> 范围：TLS 攻击、协议攻击、注入、中间人

---

## F1. TLS 相关攻击与防御

### F1.1 TLS 版本与密码套件

| 场景 | 描述 | catcher 覆盖 | 建议 |
|------|------|:----------:|------|
| TLS 1.0/1.1 服务端 | 已废弃的 TLS 版本 | ⚠️ `min_tls_version` | 默认应拒绝 TLS < 1.2 |
| NULL 密码套件 | 无加密的 TLS | ✅ rustls 默认拒绝 | 回归测试 |
| 自签名证书 | 内网开发环境 | ✅ `reject_unauthorized: false` | 验证不影响其他安全配置 |
| 证书链不完整 | 缺少中间 CA | ✅ rustls/openssl | 验证错误信息指出证书链问题 |
| 证书已过期 | 日期超期 | ✅ | 验证错误分类 (NonRetryable) |
| 证书域名不匹配 | CN/SAN 不匹配 | ✅ | 验证错误分类 (NonRetryable) |
| 证书吊销 (CRL/OCSP) | 吊销检查 | ⚠️ 依赖 TLS 库 | 默认不检查 OCSP |

### F1.2 MITM 场景

| 攻击类型 | catcher 防御 | 建议 |
|---------|-------------|------|
| ARP 欺骗 + 自签证书 | `reject_unauthorized: true` (default) | ✅ 正确拒绝 |
| 企业 TLS 中间人代理 | `ca_cert_pem` 导入企业 CA | ✅ 需测试企业 CA 链 |
| SSL Stripping (HTTP→HTTPS downgrade) | HSTS 由浏览器控制，非客户端库 | 文档建议：使用 HTTPS URL |
| 证书透明度 (CT) | CT 由浏览器/CA 生态保证 | 客户端库无需关心 |

### F1.3 TLS 指纹 (JA3/JA4)

| 场景 | 建议 |
|------|------|
| 服务端通过 TLS 指纹封禁客户端 | 文档说明 rustls vs native-tls 的 TLS 指纹不同 |
| 自定义 TLS 指纹 | 不支持，需 fork rustls（极少场景） |

---

## F2. HTTP 协议攻击

### F2.1 请求走私 (Request Smuggling)

> **注**：请求走私主要影响 HTTP 代理/网关（前端与后端对 Content-Length/Transfer-Encoding 解析不一致）。**客户端库一般不受影响**，但 catcher 的代理功能需要注意。

| 场景 | 建议 |
|------|------|
| 通过 HTTP 代理发送请求 | 代理可能被投毒，但 catcher 作为客户端不直接受影响 |
| 自定义 headers 注入 `\r\n` | 验证 headers value 不允许 CRLF (防止 HTTP Response Splitting) [1] |
| URL 中注入 `\r\n` | 验证 URL 参数过滤 CRLF |

### F2.2 SSRF (Server-Side Request Forgery)

catcher 作为客户端库，SSRF 是调用方（服务端代码）的职责。但 catcher 可以通过以下方式提高安全性：

| 场景 | 建议 |
|------|------|
| 重定向到内网 IP | 文档建议调用方实现 `beforeRedirect` 过滤内网地址 |
| DNS rebinding | 验证 DNS cache TTL 配置合理性（短 TTL → 高频解析） |

### F2.3 其他 HTTP 攻击

| 攻击 | catcher 防御 | 建议 |
|------|-------------|------|
| Host header 注入 | reqwest 从 URL 自动设置 Host | ✅ |
| Cookie 注入 | 无 cookie jar 支持 | ✅ 不自动管理 cookie |
| CRLF 注入 headers 值 | ⚠️ 未验证 | 需要在设置 headers 值时检查 `\r` `\n` |
| 超大 header (>64KB) | reqwest 默认限制 | 验证超限时的错误分类 |

---

## F3. WebSocket 安全

### F3.1 WS 握手安全

| 场景 | 建议 |
|------|------|
| Origin 检查 | 服务端职责，catcher 作为客户端不设置 Origin（由环境决定） |
| WS over TLS (wss://) | ✅ 支持 |
| WS 明文 (ws://) 降级警告 | 文档建议：生产环境始终使用 wss:// |
| WS 子协议协商失败 | 验证错误信息包含服务端返回的协议列表 |

### F3.2 WS 帧攻击

| 攻击 | tungstenite 防御 | 建议 |
|------|-----------------|------|
| 超大数据帧 (DoS) | ✅ `max_payload_bytes` | 验证超限后的连接关闭 |
| 控制帧注入 | ✅ tungstenite auto-handles Ping/Pong/Close | |
| UTF-8 非法序列 | ✅ tungstenite 验证 Text 帧 UTF-8 | |
| 帧掩码 (masking) | ✅ 客户端→服务端自动掩码 | |

---

## F4. Codec 安全

### F4.1 msgpack 反序列化安全

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| 恶意超大数据包 | ⚠️ `rmp_serde` 无大小限制 [2] | 需增加 `max_unpack_size` 限制 |
| 递归嵌套攻击 | ❌ | 需增加 `max_nesting_depth` 限制 [2] |
| 类型混淆 | ✅ serde 类型安全 | 验证反序列化失败 → DecodeError |
| ext 类型注入 | ❌ | msgpack ext 类型可能触发意外行为 |

### F4.2 JSON 解析安全

| 场景 | 建议 |
|------|------|
| JSON 炸弹 (深层嵌套) | `serde_json` 默认无限递归，需限制深度 |
| JSON 超大数据 | 无 streaming JSON 解析（与 execute_stream 不冲突） |

---

## F5. FFI 边界安全

| 场景 | catcher 覆盖 | 建议 |
|------|:----------:|------|
| null 指针检查 | ✅ 所有 FFI 入口 | 已有 ISSUE #14 修复历史 |
| `CString::new()` null byte | ✅ `replace('\0', "")` | 回归测试 |
| 缓冲区溢出 (FFI body) | ✅ body_len 字段 | 验证 `body_len` 与实际数据一致性 |
| use-after-free (callback) | ⚠️ callback 可能被多次调用 | 验证 callback 只触发一次 |
| 并发 FFI 调用 | ❌ | 验证多个线程同时调用 `catcher_http_execute` 安全性 |

---

## 阶段六总结：关键缺失

1. **CRLF 注入检查** — header value 和 URL 参数需过滤 `\r` `\n`
2. **msgpack 大小限制** — `max_unpack_size` 防止 OOM
3. **msgpack 嵌套深度限制** — 防止栈溢出
4. **JSON 解析深度限制** — `serde_json` 默认无限递归
5. **并发 FFI 调用安全** — 多线程同时调用 `catcher_http_execute`
6. **callback 单次触发** — 验证 use-after-free 不导致 callback 多调
7. **DNS rebinding 文档** — 建议调用方 `beforeRedirect` 过滤内网 IP

---

## 引用来源

1. OWASP, "CRLF Injection," https://owasp.org/www-community/vulnerabilities/CRLF_Injection ; and OWASP, "HTTP Response Splitting," https://owasp.org/www-community/attacks/HTTP_Response_Splitting
2. CVE-2024-48924, "MessagePack-CSharp DoS via hash collisions and stack overflow during deserialization," https://nvd.nist.gov/vuln/detail/CVE-2024-48924 — 同类问题适用于所有 msgpack 实现（含 Rust `rmp-serde`），攻击向量包括超大数据包和深度嵌套结构
3. RFC 7231 §6.5.7, "408 Request Timeout — keepalive race condition signal," https://datatracker.ietf.org/doc/html/rfc7231#section-6.5.7
4. Palo Alto Networks, "What Is DNS Rebinding?" https://www.paloaltonetworks.com/cyberpedia/what-is-dns-rebinding
