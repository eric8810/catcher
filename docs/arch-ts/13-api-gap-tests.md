# 13 — API Gap 功能补充测试设计

> 覆盖 [api-gap-features.md](../../issues/api-gap-features.md) G2–G12
> 遵循 [11-http-tests.md](./11-http-tests.md) 的用例编号规则和分层结构
>
> **用例编号规则**：Error `E1-Exx`，CORS `C1-Cxx`，Proxy `P1-Pxx`，FormData `F1-Fxx`，
> Redirect `RD1-RDxx`，DNS `DNS1-DNSxx`，TLS `TLS1-TLSxx`，Transport `T1-Txx`，
> Stream `ST1-STxx`，Resilience `RE1-RExx`，Auth `AU1-AUxx`。
> Rust 对应用例加 `R` 前缀（如 `RE1`、`RP1`）。

---

## 测试工具

| 平台 | 框架 | Mock 方式 |
|------|------|-----------|
| TS (http-ts) | vitest | `node:http` 创建真实 HTTP Server + `vi.spyOn` |
| TS (web) | vitest | `node:http` + fetch polyfill（如 `undici`）或 `vi.fn()` mock `globalThis.fetch` |
| Rust | `#[tokio::test]` + wiremock | wiremock MockServer |

### 测试文件结构

```
packages/catcher-http-ts/src/http/__tests__/
├── client.test.ts          # 现有 → 扩展 E1-E3, RD1-RD3, AU1-AU4
├── error-context.test.ts   # 新增 — G2 错误上下文
├── cors-cookie.test.ts     # 新增 — G3 CORS/credentials/cookie
├── proxy.test.ts           # 新增 — G4 代理
├── form-data.test.ts       # 新增 — G5 FormData
├── redirect.test.ts        # 新增 — G6 重定向
├── stream.test.ts          # 新增 — G10 流式响应
├── resilience-events.test.ts # 新增 — G11 韧性事件

packages/catcher-web/src/http/__tests__/
├── cors-cookie.test.ts     # G3 浏览器端 CORS/credentials
├── redirect.test.ts        # G6 浏览器端重定向
├── stream.test.ts          # G10 浏览器端流式
├── auth.test.ts            # G12 浏览器端 XSRF

packages/catcher-http/src/transport/__tests__/
├── error_context.rs        # G2 Rust 错误上下文
├── proxy.rs                # G4 Rust 代理
├── redirect.rs             # G6 Rust 重定向
├── dns_mapping.rs          # G7 Rust host_mapping
├── tls.rs                  # G8 Rust TLS 增强
├── transport_trait.rs      # G9 Rust Transport trait
├── stream.rs               # G10 Rust 流式
├── resilience_events.rs    # G11 Rust 韧性事件
```

---

## G2: 错误上下文丰富化

### TS 单元测试

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| E1 | 网络错误携带请求上下文 | 服务端不可达 | `error.request.url`, `error.request.method` 存在且正确 |
| E2 | HTTP 错误携带响应信息 | 返回 500 + body | `error.response.status === 500`, `error.response.data` 存在 |
| E3 | 重试错误携带 attempt 数 | 前 2 次 503 + 第 3 次 200 → 全失败 | `error.attempt === 3`（总尝试次数） |
| E4 | 超时错误携带 elapsedMs | `timeout: 100` + 服务端延迟 10s | `error.elapsedMs >= 100` |
| E5 | 取消错误类型正确 | `AbortController.abort()` | `error.type === 'cancelled'` |
| E6 | 错误分类正确 | 分别模拟 DNS/连接/TLS/HTTP 错误 | `error.type` 分别为 `'dns'/'connection'/'tls'/'http'` |
| E7 | `toJSON()` 脱敏 | headers 含 `Authorization: Bearer xxx` | JSON 字符串不含 token 值 |
| E8 | `isCatcherError()` 判断 | 正常错误 vs CatcherHttpError | 正确区分 |
| E9 | 4xx 错误也携带上下文 | 返回 403 | `error.request` + `error.response` 均存在 |

### TS 集成测试 (web)

| # | 测试名 | 断言 |
|---|--------|------|
| E10 | fetch 网络错误类型 | `error.type === 'connection'` |
| E11 | fetch HTTP 错误响应 | `error.response.status`, `error.response.data` |
| E12 | fetch 取消错误 | `error.type === 'cancelled'` |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RE1 | HttpError 携带 request 回引 | 500 | `err.request.url` 存在 |
| RE2 | 连接错误携带 context | 不可达端口 | `err.kind == ConnectionFailed`, `err.elapsed_ms > 0` |
| RE3 | 重试后 RequestError.attempt | 前 3 次 503 + 第 4 次成功 | 最终成功时 attempt 信息正确 |
| RE4 | Display 脱敏 | — | 不含 Authorization header |

---

## G3: CORS / Credentials / Cookie

### TS 单元测试 (http-ts)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| C1 | `credentials: 'include'` → axios `withCredentials: true` | `{ credentials: 'include' }` | axiosConfig 含 `withCredentials: true` |
| C2 | 请求级 `credentials` 覆盖 | 全局 `credentials: 'omit'`, 请求 `credentials: 'include'` | 该请求 `withCredentials: true` |
| C3 | `credentials: 'omit'` → axios `withCredentials: false` | `{ credentials: 'omit' }` | axiosConfig 含 `withCredentials: false` |
| C4 | 默认不传 withCredentials | 无 credentials 配置 | axiosConfig 不含 `withCredentials` |
| C4 | WS 连接传递 cookie header | `{ cookie: 'session=abc' }` | 服务端收到 `Cookie: session=abc` |

### TS 集成测试 (web)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| C5 | `credentials: 'include'` 传给 fetch | `{ credentials: 'include' }` | fetch 调用参数含 `credentials: 'include'` |
| C6 | `fetchMode: 'no-cors'` 传给 fetch | `{ fetchMode: 'no-cors' }` | fetch 调用参数含 `mode: 'no-cors'` |
| C7 | 默认 credentials 为 'same-origin' | 无配置 | fetch 调用参数含 `credentials: 'same-origin'` |
| C8 | 默认 mode 为 'cors' | 无配置 | fetch 调用参数含 `mode: 'cors'` |
| C9 | 请求级 credentials 覆盖实例 | 全局 `credentials: 'omit'`, 请求 `credentials: 'include'` | 请求级生效 |

---

## G4: 代理设置

### TS 单元测试 (http-ts)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| P1 | HTTP 代理 URL | `{ proxy: 'http://proxy:8080' }` | 请求经代理发出 |
| P2 | SOCKS5 代理 | `{ proxy: 'socks5://proxy:1080' }` | 请求经 SOCKS5 发出 |
| P3 | 代理认证 | `{ proxy: { url: '...', auth: { username, password } } }` | 代理收到认证 |
| P4 | noProxy 绕过 | `{ proxy: { url: '...', noProxy: ['localhost'] } }` | localhost 请求直连 |
| P5 | `proxy: true` 读环境变量 | `HTTP_PROXY=http://proxy:8080` | 自动使用 |
| P6 | `proxy: false` 不使用代理 | — | 直连 |
| P7 | WS 连接走代理 | `{ proxy: 'http://proxy:8080' }` | WS 经代理连接 |

### TS 集成测试

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| P8 | 代理实际转发请求 | 本地 HTTP proxy (node:http) | 请求经 proxy 到达目标 |
| P9 | 代理失败回退 | 代理不可用 | 抛出 proxy 连接错误（非超时） |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RP1 | Proxy URL 配置 | wiremock 通过代理 | 请求经代理到达 |
| RP2 | 无代理直连 | wiremock 直连 | 正常响应 |
| RP3 | 环境变量代理 | 设置 `HTTP_PROXY` | 自动使用 |
| RP4 | noProxy 绕过 | — | 匹配 noProxy 的 host 直连 |

> **代理集成测试的 Mock 策略**：
> 需要启动一个本地 HTTP proxy（可用 `http-proxy` 或简单的 `node:http` 转发），
> 后端是 `node:http` 创建的目标服务器。TS 端 `createHttpClient({ proxy: proxyUrl })`
> 指向本地 proxy。

---

## G5: FormData / 文件上传

### TS 单元测试 (http-ts)

| # | 测试名 | 输入 | 断言 |
|---|--------|------|------|
| F1 | 自动检测 FormData body | `FormData` 对象 | Content-Type 包含 `multipart/form-data` |
| F2 | 字段正确发送 | `form.append('key', 'value')` | 服务端收到字段 |
| F3 | 文件上传 | `form.append('file', buffer, 'test.txt')` | 服务端收到文件，filename 正确 |
| F4 | 多文件上传 | 多个 append | 全部收到 |
| F5 | 混合字段+文件 | field + file | 两部分都收到 |

### TS 单元测试 (web)

| # | 测试名 | 断言 |
|---|--------|------|
| F6 | 浏览器 FormData 传给 fetch | fetch 收到 FormData body，无手动 Content-Type |
| F7 | 无 body 时无 Content-Type | GET 请求 | 无 Content-Type header |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RF1 | multipart 字段 | wiremock | 请求体含字段 |
| RF2 | multipart 文件 | wiremock | 请求体含文件 + filename |
| RF3 | Content-Type 正确 | wiremock | `multipart/form-data; boundary=...` |

---

## G6: 重定向控制

### TS 单元测试 (http-ts)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| RD1 | 默认跟随重定向 | 302 → 200 | 返回 200 |
| RD2 | `follow: false` 不跟随 | `{ redirect: { follow: false } }` | 返回 302 + Location header |
| RD3 | `maxRedirects: 0` 等效禁止 | `{ redirect: { maxRedirects: 0 } }` | 返回 302 |
| RD4 | `maxRedirects` 限制 | `{ redirect: { maxRedirects: 1 } }` + 302→302→200 | 超过 1 次抛出 `MaxRedirectError` |
| RD5 | `beforeRedirect` 拦截 | `beforeRedirect` 返回 `false` | 停止在 302 |
| RD6 | `beforeRedirect` 允许 | `beforeRedirect` 返回 `true` | 继续跟随 |

### TS 单元测试 (web)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| RD7 | `redirect: 'manual'` | `{ redirect: { follow: false } }` | fetch 参数含 `redirect: 'manual'` |
| RD8 | `redirect: 'follow'` | 默认 | fetch 参数含 `redirect: 'follow'` |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RRD1 | 跟随重定向 | 302 → 200 | 返回 200 |
| RRD2 | 禁止重定向 | 302 | 返回 302 |
| RRD3 | maxRedirects 限制 | 302→302→302→... | 达到上限抛错 |

---

## G7: 自定义 Hostname 解析 (host_mapping)

### TS 单元测试

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| DNS1 | hostMapping 生效 | `{ hostMapping: { 'api.test': '127.0.0.1' } }` | 请求发送到 127.0.0.1 |
| DNS2 | hostMapping 不影响 URL | 配置 hostMapping | 最终 URL 仍含原始 hostname |
| DNS3 | 未映射 host 走正常 DNS | hostMapping 不含该 host | 正常解析 |
| DNS4 | cacheable-lookup patch 优先查表 | mock lookup 函数 + hostMapping | 映射命中时不调用原 lookup |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RDNS1 | MappingResolver 查表命中 | — | 直接返回映射 IP |
| RDNS2 | MappingResolver fallback | 映射不命中 | 走系统 DNS |
| RDNS3 | TLS SNI 保持原 hostname | mock TLS server | SNI 为原始 hostname（非映射 IP） |
| RDNS4 | 请求级 hostMapping 覆盖 | — | 请求级映射优先 |

> **host_mapping 测试的关键难点**：需要启动 HTTP 服务器在 127.0.0.1 上，
> 然后配置 `hostMapping: { 'example.com': '127.0.0.1' }`，
> 请求 `http://example.com:port/` 时实际连接到本地服务器。
> 需确保 TLS 场景下 SNI 仍为 `example.com`。

---

## G8: HTTPS 配置增强

### Rust 测试（TLS 测试需要自签证书）

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| TLS1 | CA 内联 PEM | `ca_cert_pem` | 连接成功 |
| TLS2 | mTLS 客户端证书 | `client_cert_pem` + `client_key_pem` | 服务端验证客户端成功 |
| TLS3 | 无 client_key 时 mTLS 失败 | 仅 `client_cert_pem` | 连接失败 |
| TLS4 | minTlsVersion 拒绝低版本 | `min_tls_version: 1.3` + TLS 1.2 server | 握手失败 |
| TLS5 | tlsSniOverride | SNI 为覆盖值 | 服务端收到正确 SNI |
| TLS6 | pinSha256 不匹配 | 错误的指纹 | 连接拒绝 |
| TLS7 | pinSha256 匹配 | 正确指纹 | 连接成功 |
| TLS8 | PFX 客户端身份 | `client_identity_pfx` | mTLS 成功 |

### TS 单元测试

| # | 测试名 | 断言 |
|---|--------|------|
| TLS9 | TlsConfig 类型正确传给 Agent | Agent 收到 ca/clientCert/clientKey |
| TLS10 | rejectUnauthorized=false 跳过验证 | 自签证书连接成功 |

> **TLS 测试基础设施**：需要在测试 setup 中生成自签 CA + 服务端证书 + 客户端证书。
> 可用 `openssl` 命令行或 Rust 的 `rcgen` crate 动态生成。

---

## G9: Transport trait (自定义 Adapter)

### Rust 测试

| # | 测试名 | Mock 方式 | 断言 |
|---|--------|---------|------|
| T1 | HttpTransport impl Transport | 正常请求 | trait 方法可调用 |
| T2 | 自定义 MockTransport | 注入返回固定响应的 Transport | 返回 mock 数据 |
| T3 | with_transport() 构造 | `HttpClient::with_transport(mock)` | 使用 mock 而非 reqwest |
| T4 | 默认 new() 使用 HttpTransport | `HttpClient::new(config)` | 正常行为不变 |

### TS 测试

| # | 测试名 | 断言 |
|---|--------|------|
| T5 | adapter 注入后走 adapter | 请求由 adapter 处理 | adapter.execute 被调用 |
| T6 | 无 adapter 走默认 axios | 正常请求 | axios 被调用 |

---

## G10: 流式响应

### TS 单元测试 (http-ts)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| ST1 | `responseType: 'stream'` 返回 Readable | 流式端点 | 返回值是 Node.js Readable |
| ST2 | 流式读取完整数据 | 大 body | 分块读取，最终完整 |
| ST3 | 流式与 onDownloadProgress 配合 | 大 body + `onDownloadProgress` | 回调触发多次，loaded 递增 |
| ST4 | 流式中途取消 | 大 body + AbortController | 流停止，无更多数据 |

### TS 单元测试 (web)

| # | 测试名 | 断言 |
|---|--------|------|
| ST5 | `responseType: 'stream'` 返回 ReadableStream | 返回值是 Web ReadableStream |
| ST6 | 流式读取完整数据 | 分块读取，最终完整 |

### Rust 测试

| # | 测试名 | MockServer | 断言 |
|---|--------|-----------|------|
| RST1 | ResponseBody::Stream 模式 | 大 body | 返回 Stream variant |
| RST2 | 流式读取 chunk | 分块响应 | 每个 chunk 正确到达 |
| RST3 | 流式错误处理 | 中途断开 | 流以错误终止 |

---

## G11: 韧性运行时控制

### TS 单元测试

| # | 测试名 | 操作 | 断言 |
|---|--------|------|------|
| RE1 | `on('retry', ...)` 事件触发 | 重试场景 | 事件携带 `attempt` + `error` |
| RE2 | `on('circuitBreakerChange', ...)` 触发 | 连续失败触发熔断 | `from: 'closed'`, `to: 'open'` |
| RE3 | `on('circuitBreakerChange')` 恢复 | open → halfOpen → closed | 状态链正确 |
| RE4 | `on('requestComplete', ...)` 触发 | 正常请求 | 携带 `method`, `url`, `status`, `durationMs` |
| RE5 | `on()` 返回 unsubscribe | 调用返回函数 | 不再收到事件 |
| RE6 | `off()` 取消订阅 | off 后触发事件 | listener 未调用 |
| RE7 | `updateConfig()` 修改 retry | `updateConfig({ retry: { attempts: 1 } })` | 下次请求最多重试 1 次 |
| RE8 | `updateConfig()` 仅支持 retry | 尝试修改 concurrency/CB | 抛出或忽略（不支持热更新） |
| RE9 | `updateConfig()` 不影响进行中请求 | 请求进行中调用 updateConfig | 当前请求不受影响 |

### Rust 测试

| # | 测试名 | 断言 |
|---|--------|------|
| RRE1 | RwLock<Config> 热更新 | 运行时修改配置 |
| RRE2 | mpsc 事件通道发送 | 事件正确传递到接收端 |

---

## G12: 认证辅助

### TS 单元测试 (http-ts)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| AU1 | Basic Auth 自动注入 | `{ auth: { username: 'u', password: 'p' } }` | 请求含 `Authorization: Basic dTpw` |
| AU2 | Bearer Token 自动注入 | `{ bearerToken: 'my-token' }` | 请求含 `Authorization: Bearer my-token` |
| AU3 | Bearer Token 异步刷新 | `{ bearerToken: async () => 'refreshed' }` | 每次请求调用函数 |
| AU4 | Bearer Token 缓存 | 同一 client 多次请求 | 函数被调用 N 次（不缓存） |

### TS 单元测试 (web)

| # | 测试名 | 配置 | 断言 |
|---|--------|------|------|
| AU5 | XSRF cookie → header | `{ xsrfCookieName: 'XSRF-TOKEN', xsrfHeaderName: 'X-XSRF-TOKEN' }` | 请求含 `X-XSRF-TOKEN: <value>` |
| AU6 | XSRF cookie 不存在 | 同上但无 cookie | 不注入 header |
| AU7 | Bearer Token 自动注入 | 同 AU2 | 浏览器端也生效 |

---

## 测试覆盖矩阵

| Issue | TS 单元 | TS 集成 | TS Web | Rust 单元 | Rust 集成 |
|:-----:|:-------:|:-------:|:------:|:---------:|:---------:|
| G2 错误上下文 | E1-E9 | — | E10-E12 | — | RE1-RE4 |
| G3 CORS/Cookie | C1-C4 | — | C5-C9 | — | — |
| G4 代理 | P1-P7 | P8-P9 | — | — | RP1-RP4 |
| G5 FormData | F1-F5 | — | F6-F7 | — | RF1-RF3 |
| G6 重定向 | RD1-RD6 | — | RD7-RD8 | — | RRD1-RRD3 |
| G7 host_mapping | DNS1-DNS4 | — | — | — | RDNS1-RDNS4 |
| G8 TLS 增强 | TLS9-TLS10 | — | — | — | TLS1-TLS8 |
| G9 Transport | T5-T6 | — | — | T1-T4 | — |
| G10 流式 | ST1-ST4 | — | ST5-ST6 | — | RST1-RST3 |
| G11 韧性控制 | RE1-RE9 | — | — | RRE1-RRE2 | — |
| G12 认证 | AU1-AU4 | — | AU5-AU7 | — | — |

---

## 测试基础设施需求

### 需新增的测试工具

| 工具 | 用途 | 用于 |
|------|------|------|
| `http-proxy` npm 包 | 本地 HTTP 代理服务器 | G4 Proxy |
| `socksv5` npm 包 | 本地 SOCKS5 代理服务器 | G4 Proxy |
| `rcgen` Rust crate | 动态生成 TLS 证书 | G8 TLS |
| `openssl` CLI | 备用 TLS 证书生成 | G8 TLS |
| `form-data` npm 包 | Node.js FormData 构造 | G5 FormData |

### 需新增的测试 helper

```
packages/test/helpers/
├── proxy-server.ts       # 启动/关闭本地 HTTP/SOCKS5 代理
├── tls-certs.ts          # 动态生成自签 CA + 服务端/客户端证书
├── redirect-server.ts    # 302 重定向链服务器
└── large-body-server.ts  # 返回大 body 用于流式测试
```

---

## 不测试的范围

| 不测试 | 原因 |
|--------|------|
| 真实外部代理服务器 | 需要网络，不稳定 |
| 真实 TLS 证书颁发机构 | 由 CA 生态保证 |
| 浏览器 CORS 预检行为 | 需要真实浏览器 E2E（Playwright） |
| 代理 SOCKS5 协议细节 | 由代理库保证 |
| TLS 握手密码学细节 | 由 rustls/openssl 保证 |
