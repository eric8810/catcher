# WebSocket `permessage-deflate` 服务端对接说明

本文面向服务端同事，说明 catcher 客户端启用 `perMessageDeflate = true` 后，服务端需要如何适配、如何验证，以及不支持时会发生什么。

## 背景

当前 catcher 的 websocket 客户端在 `perMessageDeflate = true` 时，会走标准 RFC 7692 的 `permessage-deflate` 扩展协商流程，目标是与 Node.js `ws` 客户端保持一致。

Flutter 项目实际调用点在：

- `lib/core/network/catcher_core_socket_transport.dart:23`

`catcher_core` 当前行为说明：

- `perMessageDeflate = true`：客户端在握手阶段发起 `Sec-WebSocket-Extensions: permessage-deflate` 协商。
- 服务端如果接受该扩展，则双方后续可以使用标准 websocket 压缩帧。
- 服务端如果不接受该扩展，连接仍然可以正常建立，消息仍按普通 websocket 帧收发，只是不会启用压缩。
- 启用标准 `permessage-deflate` 时，不会叠加 catcher 自定义的 application compression，避免双重压缩。

## 服务端需要做什么

如果服务端希望和 Flutter 客户端真正启用标准 websocket 压缩，需要支持 RFC 7692 `permessage-deflate`。

至少要满足下面几点：

1. 识别客户端握手请求中的 `Sec-WebSocket-Extensions`。
2. 当请求里包含 `permessage-deflate` 时，服务端可以按自身策略决定是否接受。
3. 如果接受，服务端在 `101 Switching Protocols` 响应头里返回：
   - `Sec-WebSocket-Extensions: permessage-deflate`
4. 后续数据帧按标准 websocket `permessage-deflate` 规则处理：
   - 压缩帧使用 RSV1 标记
   - 服务端需要能正确解压客户端压缩消息
   - 服务端发送给客户端的压缩消息也应符合 RFC 7692

## 一个最小可用的握手示例

### 客户端请求头（示意）

```http
GET /ws HTTP/1.1
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: <key>
Sec-WebSocket-Version: 13
Sec-WebSocket-Extensions: permessage-deflate
```

### 服务端接受扩展时的响应头（示意）

```http
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: <accept>
Sec-WebSocket-Extensions: permessage-deflate
```

只要服务端正确返回了 `Sec-WebSocket-Extensions: permessage-deflate`，客户端就会按标准压缩通道工作。

## 如果服务端暂时不支持怎么办

如果服务端没有实现 `permessage-deflate`：

- 不返回 `Sec-WebSocket-Extensions: permessage-deflate`
- 连接仍可成功建立
- 客户端会退回普通 websocket 收发
- 只是没有压缩收益，不会因为这个直接连接失败

这意味着服务端可以分阶段上线：

1. 先验证“不支持扩展时仍能正常通信”
2. 再上线 RFC 7692 支持
3. 最后验证压缩后的互通性和性能收益

## 服务端实现建议

### Node.js（ws）

如果服务端也是 Node.js 并使用 `ws`，通常只需要确认服务端启用了 `perMessageDeflate`。

示意：

```js
import { WebSocketServer } from 'ws';

const wss = new WebSocketServer({
  port: 8080,
  perMessageDeflate: true,
});
```

如果已有自定义参数，也要确保没有显式关闭该能力。

### Java / Go / Rust / 其他语言

不要求服务端使用 catcher，只要 websocket 服务端库支持 RFC 7692 `permessage-deflate` 即可。

对接关键不是语言，而是以下能力：

- 握手时协商 `permessage-deflate`
- 能正确处理带 RSV1 的压缩数据帧
- 能正确发送标准压缩帧给客户端

## 验收建议

建议服务端同事按下面步骤验收：

1. 抓握手包，确认客户端请求中包含：
   - `Sec-WebSocket-Extensions: permessage-deflate`
2. 服务端接受时，确认响应中返回：
   - `Sec-WebSocket-Extensions: permessage-deflate`
3. 发送一条较大的文本消息，观察服务端是否能正常解码。
4. 服务端返回一条较大的文本消息，观察 Flutter 客户端是否能正常收到并解码。
5. 验证关闭扩展协商时，连接和消息仍然正常。

## 注意事项

### 1. 不要把标准 `permessage-deflate` 和自定义业务压缩混为一谈

`permessage-deflate` 是 websocket 标准扩展，不是业务协议字段。

服务端如果接受该扩展，应由 websocket 服务器或底层库负责压缩/解压，业务层通常不应手动去处理 deflate 字节流。

### 2. 避免双重压缩

如果已经启用了标准 `permessage-deflate`，不要再在业务消息体里额外做 gzip/deflate 一层，除非你们有明确协议设计。

### 3. 关注大消息和 CPU 开销

压缩会降低带宽，但会增加 CPU 与内存开销。建议服务端关注：

- 大消息吞吐
- 长连接数量
- 压缩前后 CPU 占用
- 内存峰值

## 当前客户端侧配置说明

当前接入实现会在各平台调用处构造 `WsClientConfig`。

如果配置为：
如果这里配置为：

```dart
perMessageDeflate: true,
```

则表示客户端会主动发起标准 `permessage-deflate` 协商。

如果配置为：

```dart
perMessageDeflate: false,
```

则客户端不会主动协商该扩展。

## 对服务端同事的一句话结论

如果要和 Flutter 客户端在 `perMessageDeflate = true` 下真正启用压缩，服务端只需要支持标准 websocket RFC 7692 `permessage-deflate` 扩展协商；如果不支持，也不会影响基础连接，只是没有压缩效果。
