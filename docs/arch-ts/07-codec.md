# 07 — 编解码层

> 对应源文件：`src/codec/codec.ts`（40 行）

## 职责

提供 msgpack 二进制编解码，兼容 JSON fallback：
- 编码：JavaScript 对象 → msgpack Buffer
- 解码：msgpack Buffer → JavaScript 对象
- 自动检测：区分二进制帧 vs JSON 文本帧

## 核心导出

```typescript
import { pack, unpack, isBinary, decodeWSMessage } from 'catcher/codec'
```

### pack(value) → Buffer

```typescript
function pack(value: any): Buffer
```

任意可序列化值 → msgpack 二进制 Buffer。

```typescript
ws.send(pack({ event: 'message', data: msg }))
// → Buffer (msgpack 编码)
```

### unpack(buffer) → any

```typescript
function unpack(buffer: Buffer | Uint8Array): any
```

msgpack 二进制 → JavaScript 值。

```typescript
const data = unpack(buffer)
```

### isBinary(data) → boolean

```typescript
function isBinary(data: any): data is Buffer
```

检测 WebSocket 数据帧类型（Buffer / ArrayBuffer / Uint8Array）。

### decodeWSMessage(data) → any

```typescript
function decodeWSMessage(data: any): any
```

自动检测并解码 WebSocket 消息帧：
- 二进制帧 → msgpack 解码
- 文本帧 → JSON.parse（fallback）
- 其他 → 原样返回

```typescript
ws.addEventListener('message', (e) => {
  const data = decodeWSMessage(e.data)
  // 无需关心是 binary 还是 text 帧
})
```

## 实现细节

底层使用 `msgpackr` 库，2-4x 快于 JSON，体积减少 47%。

```
JSON:        {"event":"msg","data":{"text":"hello"}}
             → 41 bytes

msgpack:     82 a5 65 76 65 6e 74 a3 6d 73 67 ...
             → ~22 bytes (-46%)
```
