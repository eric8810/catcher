# 常见问题

> catcher 使用中常见问题及解决方案。

---

## 安装与配置

### Q: 我应该用 napi 版还是 TS 版？

| 需求 | 推荐 |
|------|------|
| 追求极致性能、低 CPU 开销 | napi 版（`@eric8810/catcher-napi-http`） |
| 需要动态拦截器（auth token 注入等） | TS 版（`@eric8810/catcher-http`） |
| Electron 主进程 | napi 或 TS 均可 |
| Electron 渲染进程 | TS 版（napi 原生模块不加载） |

### Q: napi 包安装失败

```bash
# 确保 Node.js >= 18
node --version

# 确保安装了 Rust toolchain
rustup show

# napi-rs 预编译的 .node 文件
# Linux x64: linux-x64-gnu
# macOS arm64: darwin-arm64
# Windows x64:  win32-x64-msvc
```

### Q: 安装后 import 报错 "Cannot find module"

```bash
# 检查是否同时安装了 catcher-core（TS 类型依赖）
npm ls @eric8810/catcher-core
# 如果缺失
npm install @eric8810/catcher-core
```

---

## 重试与韧性

### Q: 为什么我的请求没有被重试？

**可能原因 1：4xx 错误不会重试**

catcher 默认只重试网络错误和 5xx。4xx 错误被视为客户端错误（如 400/401/403/404），重试它们没有意义。

需要重试特定 4xx（如 429 Rate Limit）时，自定义 `retryIf`：

```typescript
const client = createHttpClient({
  baseURL: '...',
  retry: {
    attempts: 3,
    retryIf: (error) => {
      return isCatcherError(error) &&
        (error.type === 'timeout' || error.response?.status === 429)
    },
  },
})
```

**可能原因 2：keepAlive 连接复用问题**

catcher 在重试时会自动销毁空闲 keepAlive 连接，强制使用新连接。如果仍有问题，可以完全禁用 keepAlive：

```typescript
createHttpClient({ baseURL: '...', keepAlive: false })
```

### Q: 如何在特定请求上关闭重试？

```typescript
// 某个查询幂等性不重要，不希望重试
await client.get('/non-critical', { retry: false })
```

### Q: 熔断器一直处于 OPEN 状态怎么办？

熔断器 OPEN 后会等待 `resetTimeout` 后自动进入 HALF_OPEN。如果需要立即重置：

```typescript
// 检查状态
console.log(client.circuitBreakerState())  // 'open'

// 无法手动重置 — 熔断器会自动恢复。调整 resetTimeout 更短：
createHttpClient({
  circuitBreaker: { failureThreshold: 5, resetTimeout: 10_000 }  // 10s 恢复
})
```

---

## 代理与网络

### Q: 如何配置代理？

```typescript
// 环境变量自动检测
createHttpClient({
  baseURL: '...',
  proxy: true,  // 自动读取 HTTPS_PROXY/HTTP_PROXY
})

// 显式指定
createHttpClient({
  proxy: 'http://proxy.company.com:8080',
})

// SOCKS5 代理
createHttpClient({
  proxy: 'socks5://127.0.0.1:1080',
})

// 需要安装相应的代理包
npm install https-proxy-agent socks-proxy-agent
```

### Q: proxy: true 不工作

1. 检查环境变量已设置：`echo $HTTPS_PROXY`
2. 确保已安装代理包：`npm install https-proxy-agent`
3. 使用显式 URL 而非 `true`

### Q: 如何忽略特定域名的代理？

```typescript
createHttpClient({
  proxy: {
    url: 'http://proxy:8080',
    noProxy: ['localhost', '*.internal.com'],
  },
})
```

---

## TLS / 证书

### Q: 开发环境证书错误 (self-signed cert)

```typescript
// 仅开发测试用，生产环境务必保持 true
createHttpClient({ rejectUnauthorized: false })
```

### Q: 如何配置客户端证书 (mTLS)？

```typescript
createHttpClient({
  tls: {
    clientCertPem: fs.readFileSync('client.crt', 'utf8'),
    clientKeyPem: fs.readFileSync('client.key', 'utf8'),
  },
})
```

---

## WebSocket

### Q: WebSocket 连接频繁断开重连

检查是否触发了 `maxPayload` 限制（默认 1MB）：

```typescript
createResilientWS({
  url: ['wss://...'],
  maxPayload: 5 * 1024 * 1024,  // 5MB
})
```

### Q: 如何只连接单个端点？

```typescript
// 单个 URL 字符串即可，不需要数组
createResilientWS({ url: 'wss://api.example.com' })
```

### Q: 如何禁用自动重连？

```typescript
createResilientWS({
  url: ['wss://...'],
  reconnect: { maxAttempts: 0 },
})
```

---

## SSE

### Q: SSE 流没有数据输出

1. 检查 Content-Type 头：服务端必须返回 `text/event-stream`
2. 检查 CORS（浏览器端）：服务端需要 `Access-Control-Allow-Origin`
3. 使用 `createSSEClient` 而非 `createSSEStream`（长连接场景）

### Q: SSE 流中断后如何续传？

`createSSEClient` 会自动发送 `Last-Event-ID` 头。确保服务端支持该语义。

### Q: Browser SSE 报 CORS 错误

```typescript
// 在 createSSEClient 中不要设置 mode — 由浏览器自动处理
// 确保服务端返回：
// Access-Control-Allow-Origin: *
// Access-Control-Allow-Headers: Last-Event-ID
```

---

## Flutter / Dart

### Q: Flutter 运行时找不到动态库

```bash
# Android: 确保 libcatcher_ffi.so 打包进了 APK
flutter build apk  # native_assets 自动处理

# iOS: 确保 .a 文件链接正确
cd ios && pod install

# macOS/Linux/Windows: 确保 .dylib/.so/.dll 在可搜索路径
```

### Q: pub.dev 版本落后于 npm

pub.dev 包使用独立的 `catcher_core-v*` tag 发布。确认 tag 已推送：

```bash
git tag -l catcher_core-v*
```

---

## 浏览器

### Q: 浏览器中 keepAlive 不生效

浏览器的 HTTP 连接池由浏览器自主管理，`keepAlive` 选项对浏览器无效。

### Q: 浏览器中 WebSocket 代理不生效

浏览器的 WebSocket 不支持 SOCKS5 代理。TLS 证书校验由浏览器控制。

---

## Electron

### Q: 渲染进程不能使用 napi 包

Electron 渲染进程无法加载原生 Node.js 模块。主进程用 napi，渲染进程用 `contextBridge` + IPC 桥接。

```typescript
// 主进程
ipcMain.handle('api:get', async (_e, url) => client.get(url))

// 渲染进程（通过 preload.ts）
const data = await window.api.get('/data')
```

### Q: Electron 打包后找不到 .node 文件

确保 napi 包被标记为 external，不被 webpack/vite 打包：

```javascript
// electron-builder 配置
externals: ['@eric8810/catcher-napi-http', '@eric8810/catcher-napi-ws']
```

---

## Rust

### Q: Cargo.toml 中 catcher 版本约束应该怎么写？

catcher 遵循语义化版本。建议写完整版本或兼容性约束：

```toml
catcher-http = "0.3.10"       # 精确兼容 MAJOR.MINOR
# 或
catcher-http = ">=0.2,<0.3"  # 0.2.x 系列
```

### Q: Rust crate 依赖 tokio，如何选择 runtime？

catcher 内部使用 `tokio::spawn`，需要 tokio runtime。确保你的 `main` 函数有 `#[tokio::main]` 注解，或使用 `tokio::runtime::Runtime` 手动管理。
