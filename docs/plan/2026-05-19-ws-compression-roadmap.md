# WebSocket Compression Roadmap

## Goal

Support compressed WebSocket payloads for Flutter/Rust clients with the same standard RFC 7692 `permessage-deflate` wire protocol used by the Node.js `ws` client.

## Implemented Now: Standard `permessage-deflate`

Rust/Flutter/napi WS transport now uses `yawc`, which negotiates `Sec-WebSocket-Extensions: permessage-deflate` and automatically compresses/decompresses RSV1 data frames.

Client behavior:

- `per_message_deflate` / `perMessageDeflate` defaults to `true`, matching the Node.js `ws` path.
- Compression level is 6.
- If the server does not negotiate the extension, messages remain valid normal WebSocket frames.
- Application-layer gzip/zstd is skipped when standard permessage-deflate is enabled to avoid double compression.

Validation:

- `permessage_deflate_test` verifies that the client sends a `permessage-deflate` offer and sets RSV1 on the first data frame after server negotiation.

## Fallback: Application Compression

Rust/Flutter clients can enable `application_compression` / `applicationCompression`.

Wire format:

```text
bytes 0..12   "CATCHER-CMP-1"
byte 13       algorithm: 1 = gzip, 2 = zstd
byte 14       original kind: 1 = text, 2 = binary
bytes 15..18  uncompressed length, uint32 big-endian
bytes 19..    compressed payload
```

Client behavior:

- Outbound text/binary messages at or above `threshold_bytes` are compressed and sent as binary frames.
- Outbound messages below the threshold stay as normal text/binary frames.
- Inbound binary frames with this envelope are decompressed and emitted with the original `is_binary` value.
- Inbound normal text/binary frames remain compatible.
- Gzip uses level 6; zstd uses level 3.

Server adaptation checklist:

- Read `X-Catcher-Application-Compression`, `X-Catcher-Application-Compression-Format`, and `X-Catcher-Application-Compression-Threshold` during the WebSocket handshake.
- Detect `CATCHER-CMP-1` before normal binary decoding.
- Reject frames whose uncompressed length exceeds server policy.
- Decompress based on algorithm byte.
- Route kind `1` as text UTF-8 bytes and kind `2` as binary bytes.
- When sending compressed responses, use the same envelope.

## Remaining Acceptance Criteria

- Text and binary compressed frames roundtrip through Flutter FFI against the production server.
- Existing reconnect, heartbeat, headers, protocols, TLS, and multi-endpoint behavior still pass.
- Autobahn or equivalent RFC 6455/RFC 7692 conformance tests pass.
- Monitor memory/CPU under long-lived compressed connections.
