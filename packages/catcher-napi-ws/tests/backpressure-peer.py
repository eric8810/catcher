"""One-shot real TCP/WS peer with a bounded receive window for Windows tests."""
import base64
import hashlib
import json
import socket
import sys

listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
listener.bind(('127.0.0.1', 0))
listener.listen(1)
print(json.dumps({'event': 'listening', 'port': listener.getsockname()[1]}), flush=True)
peer, _ = listener.accept()
peer.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
request = bytearray()
while not request.endswith(b'\r\n\r\n'):
    chunk = peer.recv(1)
    if not chunk:
        raise RuntimeError('connection ended before upgrade')
    request.extend(chunk)
headers = dict(line.split(':', 1) for line in request.decode().split('\r\n')[1:] if ':' in line)
key = next(value.strip() for name, value in headers.items() if name.lower() == 'sec-websocket-key')
accept = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
peer.sendall(('HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ' + accept + '\r\n\r\n').encode())
first = bytearray()
while len(first) < 14:
    chunk = peer.recv(14 - len(first))
    if not chunk:
        raise RuntimeError('binary frame did not start')
    first.extend(chunk)
print(json.dumps({'event': 'started', 'header': first.hex()}), flush=True)
if sys.stdin.readline().strip() != 'resume':
    raise RuntimeError('missing resume command')
received = len(first)
payload_end = 14 + int.from_bytes(first[2:10], "big")
after_payload = bytearray()
while True:
    try:
        chunk = peer.recv(65536)
    except ConnectionResetError:
        # Interrupted sends may terminate TCP with RST instead of FIN.
        break
    if not chunk:
        break
    previous = received
    received += len(chunk)
    if received > payload_end:
        after_payload.extend(chunk[max(0, payload_end - previous):])
print(json.dumps({'event': 'done', 'bytes': received, 'afterPayload': after_payload.hex()}), flush=True)
peer.close()
listener.close()
