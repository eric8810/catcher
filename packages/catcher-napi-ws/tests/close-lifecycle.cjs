// Run against a locally built native addon, never a mock or published binary.
// CATCHER_WS_ADDON=/absolute/path/addon.node node --test tests/close-lifecycle.cjs
const { test } = require('node:test')
const assert = require('node:assert/strict')
const http = require('node:http')
const crypto = require('node:crypto')
const { once } = require('node:events')
const path = require('node:path')
const fs = require('node:fs')
const root = path.resolve(__dirname, '..')
const addon = process.env.CATCHER_WS_ADDON || path.join(root, fs.readdirSync(root).find(file => file.endsWith('.node')) || 'missing.node')
const { JsWsClient } = require(addon)
const delay = ms => new Promise(resolve => setTimeout(resolve, ms))

async function fixture(run) {
  const server = http.createServer()
  const sockets = new Set()
  server.on('connection', socket => {
    sockets.add(socket)
    socket.on('close', () => sockets.delete(socket))
    socket.on('end', () => socket.destroy())
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const config = {
    urls: [`ws://127.0.0.1:${server.address().port}`],
    handshake_timeout_ms: 60000,
    reconnect: { initial_delay_ms: 30, max_delay_ms: 30, max_attempts: 3 },
    per_message_deflate: false,
  }
  try { await run(server, config) }
  finally {
    for (const socket of sockets) socket.destroy()
    await new Promise(resolve => server.close(resolve))
  }
}
function upgrade(req, socket) {
  const accept = crypto.createHash('sha1')
    .update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64')
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`)
}

test('close before initial handshake stops the actual native instance', { timeout: 5000 }, async () => {
  await fixture(async (server, config) => {
    let upgrades = 0
    const events = []
    const accepted = once(server, 'upgrade')
    server.on('upgrade', () => upgrades++)
    const client = new JsWsClient(JSON.stringify(config), (err, value) => {
      assert.ifError(err); events.push(JSON.parse(value))
    })
    const [, socket] = await accepted
    const ended = once(socket, 'close')
    client.close(null, null)
    client.close(null, null)
    await client.closeAndWait(null, null)
    await ended
    await delay(150)
    assert.equal(upgrades, 1)
    assert.deepEqual(events, [])
    assert.throws(() => client.send('late'))
    await client.closeAndWait(null, null)
  })
})

test('close during reconnect cancels handshake and discards cached sends', { timeout: 5000 }, async () => {
  await fixture(async (server, config) => {
    let upgrades = 0, reconnectSocket
    let resolveReconnect, resolveDisconnected
    const reconnect = new Promise(resolve => { resolveReconnect = resolve })
    const disconnected = new Promise(resolve => { resolveDisconnected = resolve })
    const received = []
    const events = []
    server.on('upgrade', (req, socket) => {
      upgrades++
      if (upgrades === 1) {
        upgrade(req, socket)
        setTimeout(() => socket.destroy(), 20)
      } else {
        reconnectSocket = socket
        socket.on('data', data => received.push(data))
        resolveReconnect()
      }
    })
    const client = new JsWsClient(JSON.stringify(config), (err, value) => {
      assert.ifError(err)
      const event = JSON.parse(value); events.push(event)
      if (event.type === 'Disconnected') resolveDisconnected()
    })
    await disconnected
    client.send('buffered text')
    client.sendBinary(Buffer.from('buffered binary'))
    await reconnect
    const ended = once(reconnectSocket, 'close')
    client.close(1000, 'stop')
    await client.closeAndWait(null, null)
    await ended
    const count = events.length
    await delay(150)
    assert.equal(events.length, count)
    assert.equal(events.filter(event => event.type === 'Connected').length, 1)
    assert.equal(upgrades, 2)
    assert.equal(Buffer.concat(received).length, 0)
    client.close(null, null)
  })
})

test('connected close preserves code and reason and remains idempotent', { timeout: 5000 }, async () => {
  await fixture(async (server, config) => {
    const frames = []
    let resolveConnected
    const connected = new Promise(resolve => { resolveConnected = resolve })
    let ended
    server.on('upgrade', (req, socket) => {
      ended = once(socket, 'close')
      socket.on('data', data => frames.push(data))
      upgrade(req, socket)
    })
    const client = new JsWsClient(JSON.stringify(config), (err, value) => {
      assert.ifError(err)
      if (JSON.parse(value).type === 'Connected') resolveConnected()
    })
    await connected
    await client.closeAndWait(1001, 'going away')
    await ended
    const frame = Buffer.concat(frames)
    assert.equal(frame[0] & 15, 8)
    const length = frame[1] & 127
    const payload = Buffer.from(frame.subarray(6, 6 + length))
    for (let i = 0; i < payload.length; i++) payload[i] ^= frame[2 + i % 4]
    assert.equal(payload.readUInt16BE(0), 1001)
    assert.equal(payload.subarray(2).toString(), 'going away')
    await client.closeAndWait(null, null)
  })
})

test('GC of native wrapper cancels an unfinished handshake', { timeout: 5000, skip: !global.gc }, async () => {
  await fixture(async (server, config) => {
    let upgrades = 0
    server.on('upgrade', () => upgrades++)
    const accepted = once(server, 'upgrade')
    let client = new JsWsClient(JSON.stringify(config))
    const weak = new WeakRef(client)
    const [, socket] = await accepted
    const ended = once(socket, 'close')
    client = null
    for (let i = 0; i < 50; i++) {
      await delay(10)
      global.gc()
      await delay(10)
      if (!weak.deref()) break
    }
    assert.equal(weak.deref(), undefined)
    await ended
    await delay(150)
    assert.equal(upgrades, 1)
  })
})

async function boundedReceivePeer() {
  const { spawn } = require('node:child_process')
  const { createInterface } = require('node:readline')
  const peer = spawn(process.platform === 'win32' ? 'python' : 'python3',
    ['-u', path.join(__dirname, 'backpressure-peer.py')], { stdio: ['pipe', 'pipe', 'pipe'] })
  const messages = new Map(), waiters = new Map()
  let failure
  const rejectAll = error => {
    failure = error
    for (const waiter of waiters.values()) waiter.reject(error)
  }
  peer.on('error', rejectAll)
  peer.stderr.on('data', data => rejectAll(new Error(data.toString())))
  createInterface({ input: peer.stdout }).on('line', line => {
    const message = JSON.parse(line)
    messages.set(message.event, message)
    waiters.get(message.event)?.resolve(message)
  })
  return {
    wait: event => failure ? Promise.reject(failure) : messages.has(event)
      ? Promise.resolve(messages.get(event))
      : new Promise((resolve, reject) => waiters.set(event, { resolve, reject })),
    resume: () => peer.stdin.write('resume\n'),
    close: () => peer.kill(),
  }
}

// Exercise both native exports and the public TS facade with each transport backend.
for (const backend of ['native', 'reqwest']) {
  for (const api of ['raw', 'ts']) {
    const create = (config, onEvent) => {
      if (backend === 'reqwest') config.proxy = { mode: 'direct' }
      if (api === 'ts') {
        const { WsClient } = require('../dist/client.js')
        return new WsClient(config, onEvent)
      }
      return new JsWsClient(JSON.stringify(config), (err, value) => {
        assert.ifError(err); onEvent(JSON.parse(value))
      })
    }

    test(`${backend}/${api}: closeAndWait rejects sends synchronously and preserves idle Close`, { timeout: 5000 }, async () => {
      for (let iteration = 0; iteration < 20; iteration++) {
      await fixture(async (server, config) => {
        const frames = []
        let resolveConnected, ended
        const connected = new Promise(resolve => { resolveConnected = resolve })
        server.on('upgrade', (req, socket) => {
          ended = once(socket, 'close')
          socket.on('data', data => frames.push(data))
          upgrade(req, socket)
        })
        const client = create(config, event => {
          if (event.type === 'Connected') resolveConnected()
        })
        try {
          await connected
          const waiting = client.closeAndWait(1001, 'stop')
          assert.throws(() => client.send('late'), /closed/)
          assert.throws(() => client.sendBinary(Buffer.from('late')), /closed/)
          assert.throws(() => client.networkChanged(), /closed/)
          await waiting
          await ended
          const frame = Buffer.concat(frames)
          assert.equal(frame[0] & 15, 8)
          const length = frame[1] & 127
          const payload = Buffer.from(frame.subarray(6, 6 + length))
          for (let i = 0; i < payload.length; i++) payload[i] ^= frame[2 + i % 4]
          assert.equal(payload.readUInt16BE(0), 1001)
          assert.equal(payload.subarray(2).toString(), 'stop')
          await client.closeAndWait(null, null)
        } finally { client.close(null, null) }
      })
      }
    })

    test(`${backend}/${api}: cancelled in-flight payload is never flushed by Close`, { timeout: 10000 }, async () => {
      if (process.platform === 'win32' || process.env.CATCHER_BOUNDED_RECEIVE_TEST) {
        const peer = await boundedReceivePeer()
        let client, timer
        try {
          const { port } = await peer.wait('listening')
          let resolveConnected
          const connected = new Promise(resolve => { resolveConnected = resolve })
          client = create({ urls: [`ws://127.0.0.1:${port}`], per_message_deflate: false, send_timeout_ms: 0 }, event => {
            if (event.type === 'Connected') resolveConnected()
          })
          await connected
          global.gc?.()
          const payload = crypto.randomBytes(32 * 1024 * 1024)
          client.sendBinary(payload)
          const { header } = await peer.wait('started')
          const frame = Buffer.from(header, 'hex')
          assert.equal(frame[0] & 15, 2)
          assert.equal(Number(frame.readBigUInt64BE(2)), payload.length)
          await delay(100)
          client.close(1001, 'stop')
          const waiting = client.closeAndWait(null, null)
          timer = setTimeout(() => peer.resume(), 10)
          await waiting
          const { bytes } = await peer.wait('done')
          assert.ok(bytes < payload.length + 14, `cancelled send was flushed: received ${bytes} bytes`)
        } finally {
          clearTimeout(timer)
          client?.close(null, null)
          peer.close()
        }
        return
      }
      await fixture(async (server, config) => {
        config.send_timeout_ms = 0
        const chunks = []
        let resolveConnected, resolveStarted, socket, ended
        const connected = new Promise(resolve => { resolveConnected = resolve })
        const started = new Promise(resolve => { resolveStarted = resolve })
        server.on('upgrade', (req, peer) => {
          socket = peer
          ended = once(peer, 'close')
          peer.on('data', data => {
            chunks.push(data)
            // Pause as soon as the binary frame starts, ensuring send/flush is in flight.
            peer.pause()
            resolveStarted()
          })
          upgrade(req, peer)
        })
        const client = create(config, event => {
          if (event.type === 'Connected') resolveConnected()
        })
        let resume
        try {
          await connected
          global.gc?.()
          const payload = crypto.randomBytes(32 * 1024 * 1024)
          client.sendBinary(payload)
          await started
          await delay(100)
          assert.ok(Buffer.concat(chunks).length < payload.length)
          client.close(1001, 'stop')
          const waiting = client.closeAndWait(null, null)
          // A Close that repolls the cancelled sink will flush the full frame here.
          resume = setTimeout(() => {
            socket.removeAllListeners('data')
            socket.on('data', data => chunks.push(data))
            socket.resume()
          }, 10)
          await waiting
          await ended
          const received = Buffer.concat(chunks)
          assert.equal(received[0] & 15, 2)
          assert.equal(Number(received.readBigUInt64BE(2)), payload.length)
          assert.ok(received.length < payload.length + 14,
            `cancelled send was flushed: received ${received.length} bytes`)
        } finally {
          clearTimeout(resume)
          client.close(null, null)
        }
      })
    })
  }
}
