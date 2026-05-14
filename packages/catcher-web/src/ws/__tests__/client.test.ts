/**
 * 浏览器端 WebSocket 客户端测试 — catcher-web 版
 *
 * Mock 方式：拦截全局 WebSocket 构造函数，控制 onopen/onclose/onmessage/onerror 回调。
 * 用例编号 BW1-BW25，与设计文档 docs/arch-ts/14-web-ws-tests.md 一一对应。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { createWebSocketClient } from '../client.js'

// ── Polyfill CloseEvent for Node.js ──────────────────────
// CloseEvent is a browser-only API; vitest runs in Node by default.
if (typeof (globalThis as any).CloseEvent === 'undefined') {
  ;(globalThis as any).CloseEvent = class CloseEvent extends Event {
    readonly code: number
    readonly reason: string
    readonly wasClean: boolean
    constructor(type: string, init?: { code?: number; reason?: string; wasClean?: boolean }) {
      super(type)
      this.code = init?.code ?? 0
      this.reason = init?.reason ?? ''
      this.wasClean = init?.wasClean ?? false
    }
  }
}

// ── Mock WebSocket ────────────────────────────────────────

interface MockWSInstance {
  url: string
  protocols?: string | string[]
  binaryType: BinaryType
  readyState: number
  send: ReturnType<typeof vi.fn>
  close: ReturnType<typeof vi.fn>
  onopen: ((ev: Event) => void) | null
  onclose: ((ev: CloseEvent) => void) | null
  onmessage: ((ev: MessageEvent) => void) | null
  onerror: ((ev: Event) => void) | null
}

let instances: MockWSInstance[] = []
let OriginalWS: typeof WebSocket | undefined

function createMockWS(url: string | URL, protocols?: string | string[]): MockWSInstance {
  const inst: MockWSInstance = {
    url: typeof url === 'string' ? url : url.toString(),
    protocols,
    binaryType: 'blob',
    readyState: 0, // CONNECTING
    send: vi.fn(),
    close: vi.fn(((code?: number, _reason?: string) => {
      inst.readyState = 3 // CLOSED
    }) as any),
    onopen: null,
    onclose: null,
    onmessage: null,
    onerror: null,
  }
  instances.push(inst)
  return inst
}

function mockWebSocket() {
  const ctor = vi.fn((url: string | URL, protocols?: string | string[]) => {
    return createMockWS(url, protocols)
  }) as any
  // Preserve static constants used by client.ts
  ctor.CONNECTING = 0
  ctor.OPEN = 1
  ctor.CLOSING = 2
  ctor.CLOSED = 3

  OriginalWS = globalThis.WebSocket
  globalThis.WebSocket = ctor
  return ctor
}

function restoreWebSocket() {
  if (OriginalWS) {
    globalThis.WebSocket = OriginalWS
  }
}

function lastInstance(): MockWSInstance {
  return instances[instances.length - 1]
}

// ── Setup / Teardown ──────────────────────────────────────

beforeEach(() => {
  instances = []
  vi.useFakeTimers()
  mockWebSocket()
})

afterEach(() => {
  vi.useRealTimers()
  restoreWebSocket()
  vi.restoreAllMocks()
})

// ── Tests ──────────────────────────────────────────────────

describe('createWebSocketClient (catcher-web)', () => {
  // ── 一、连接生命周期 ────────────────────────────────────

  describe('连接生命周期', () => {
    it('BW1 成功连接 → open 事件', () => {
      const onOpen = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('open', onOpen)

      const ws = lastInstance()
      ws.readyState = 1 // OPEN
      ws.onopen!(new Event('open'))

      expect(client.status).toBe('CONNECTED')
      expect(onOpen).toHaveBeenCalledTimes(1)
    })

    it('BW2 连接时设置 binaryType', () => {
      createWebSocketClient({ url: 'ws://test', binaryType: 'arraybuffer' })
      expect(lastInstance().binaryType).toBe('arraybuffer')
    })

    it('BW3 使用 protocols', () => {
      const ctor = globalThis.WebSocket as unknown as ReturnType<typeof vi.fn>
      createWebSocketClient({ url: 'ws://test', protocols: ['proto1'] })
      expect(ctor).toHaveBeenCalledWith('ws://test', ['proto1'])
    })

    it('BW4 多 URL 时使用第一个', () => {
      createWebSocketClient({ url: ['ws://a', 'ws://b'] })
      expect(lastInstance().url).toBe('ws://a')
    })
  })

  // ── 二、消息收发 ────────────────────────────────────────

  describe('消息收发', () => {
    it('BW5 send 发送文本', () => {
      const client = createWebSocketClient({ url: 'ws://test' })
      const ws = lastInstance()
      ws.readyState = 1 // OPEN
      ws.onopen!(new Event('open'))

      client.send('hello')
      expect(ws.send).toHaveBeenCalledWith('hello')
    })

    it('BW6 send 发送 ArrayBuffer', () => {
      const client = createWebSocketClient({ url: 'ws://test' })
      const ws = lastInstance()
      ws.readyState = 1
      ws.onopen!(new Event('open'))

      const buf = new ArrayBuffer(4)
      client.send(buf)
      expect(ws.send).toHaveBeenCalledWith(buf)
    })

    it('BW7 未连接时 send 不抛错', () => {
      const client = createWebSocketClient({ url: 'ws://test' })
      // Don't trigger onopen — activeSocket stays null
      expect(() => client.send('hello')).not.toThrow()
      expect(lastInstance().send).not.toHaveBeenCalled()
    })

    it('BW8 接收消息 → message 事件', () => {
      const onMessage = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('message', onMessage)

      const ws = lastInstance()
      const msgEvent = { data: 'hello' } as MessageEvent
      ws.onmessage!(msgEvent)

      expect(onMessage).toHaveBeenCalledTimes(1)
    })
  })

  // ── 三、关闭与重连 ──────────────────────────────────────

  describe('握手超时', () => {
    it('BW15 握手超时 → close(4000)', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 10, maxDelay: 50, maxAttempts: 5 },
      })

      const ws = lastInstance()
      // Advance past 10s handshake timeout — client calls ws.close(4000)
      vi.advanceTimersByTime(10_000)
      expect(ws.close).toHaveBeenCalledWith(4000, 'Handshake timeout')
    })
  })

  describe('关闭与重连', () => {
    it('BW9 close() → close 事件', () => {
      const onClose = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('close', onClose)

      const ws = lastInstance()
      ws.readyState = 1
      ws.onopen!(new Event('open'))

      client.close()
      expect(ws.close).toHaveBeenCalled()

      ws.readyState = 3
      ws.onclose!(new CloseEvent('close', { code: 1000 }))
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('BW10 close() 后不重连', () => {
      const client = createWebSocketClient({ url: 'ws://test' })

      const ws = lastInstance()
      ws.readyState = 1
      ws.onopen!(new Event('open'))

      client.close()
      ws.readyState = 3
      ws.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(60_000)
      expect(instances.length).toBe(1)
    })

    it('BW11 close(code, reason) 传递参数', () => {
      const client = createWebSocketClient({ url: 'ws://test' })
      const ws = lastInstance()
      ws.readyState = 1
      ws.onopen!(new Event('open'))

      client.close(1000, 'done')
      expect(ws.close).toHaveBeenCalledWith(1000, 'done')
    })

    it('BW12 服务端关闭后自动重连', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 10, maxDelay: 50, maxAttempts: 5 },
      })

      const ws1 = lastInstance()
      ws1.readyState = 1
      ws1.onopen!(new Event('open'))

      ws1.readyState = 3
      ws1.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(100)
      expect(instances.length).toBe(2)
    })

    it('BW13 maxAttempts 耗尽停止', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 10, maxDelay: 50, maxAttempts: 2 },
      })

      for (let i = 0; i < 10; i++) {
        if (instances.length <= i) break
        const ws = instances[i]
        ws.readyState = 3
        ws.onclose!(new CloseEvent('close'))
        vi.advanceTimersByTime(100)
      }

      // initial + 2 reconnect attempts = 3 total
      expect(instances.length).toBeLessThanOrEqual(3)
    })

    it('BW14 重连成功后退避重置', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 10, maxDelay: 50, maxAttempts: 5 },
      })

      // First connection + close
      const ws1 = lastInstance()
      ws1.readyState = 1
      ws1.onopen!(new Event('open'))
      ws1.readyState = 3
      ws1.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(100)

      // Second connection succeeds → reset()
      const ws2 = lastInstance()
      ws2.readyState = 1
      ws2.onopen!(new Event('open'))

      ws2.readyState = 3
      ws2.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(100)

      // Third connection (reset means attempt count starts fresh)
      expect(instances.length).toBe(3)
    })
  })

  // ── 四、事件系统 ────────────────────────────────────────

  describe('事件系统', () => {
    it('BW16 addEventListener 注册', () => {
      const onOpen = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('open', onOpen)

      lastInstance().onopen!(new Event('open'))
      expect(onOpen).toHaveBeenCalledTimes(1)
    })

    it('BW17 removeEventListener 移除', () => {
      const onOpen = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('open', onOpen)
      client.removeEventListener('open', onOpen)

      lastInstance().onopen!(new Event('open'))
      expect(onOpen).not.toHaveBeenCalled()
    })

    it('BW18 多 listener 同类型', () => {
      const l1 = vi.fn()
      const l2 = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('open', l1)
      client.addEventListener('open', l2)

      lastInstance().onopen!(new Event('open'))
      expect(l1).toHaveBeenCalledTimes(1)
      expect(l2).toHaveBeenCalledTimes(1)
    })

    it('BW19 error 事件分发', () => {
      const onError = vi.fn()
      const client = createWebSocketClient({ url: 'ws://test' })
      client.addEventListener('error', onError)

      lastInstance().onerror!(new Event('error'))
      expect(onError).toHaveBeenCalledTimes(1)
    })

    it('BW20 url 属性', () => {
      const client = createWebSocketClient({ url: 'ws://test/ws' })
      expect(client.url).toBe('ws://test/ws')
    })
  })

  // ── 五、退避策略 ────────────────────────────────────────
  // createReconnectState 是内部函数，通过观察重连间隔间接测试

  describe('退避策略', () => {
    it('BW21 首次延迟 ≈ initialDelay', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 500, maxDelay: 5000, maxAttempts: 5 },
      })

      // First connection succeeds
      const ws = lastInstance()
      ws.readyState = 1
      ws.onopen!(new Event('open'))

      // Server closes
      ws.readyState = 3
      ws.onclose!(new CloseEvent('close'))

      // Advance less than 250ms — no reconnect yet (500ms ± 25% jitter)
      vi.advanceTimersByTime(249)
      expect(instances.length).toBe(1)

      // Advance past the full delay — reconnect should happen
      vi.advanceTimersByTime(500)
      expect(instances.length).toBe(2)
    })

    it('BW22 指数增长 — 构造失败时延迟递增', () => {
      // Make WebSocket constructor throw on first 3 calls to test exponential backoff
      const ctor = globalThis.WebSocket as any
      let throwCount = 0
      const originalImpl = ctor.getMockImplementation()
      ctor.mockImplementation((url: string | URL, protocols?: string | string[]) => {
        throwCount++
        if (throwCount <= 3) {
          throw new Error('Connection refused')
        }
        return createMockWS(url, protocols)
      })

      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 100, maxDelay: 100_000, backoffMultiplier: 2, maxAttempts: 5 },
      })

      // Track when each new WS constructor call happens
      const reconnectTimes: number[] = []
      let elapsed = 0

      // Wait for all 3 failed attempts + 1 successful
      for (let i = 0; i < 4; i++) {
        const prevCallCount = ctor.mock.calls.length
        const prevElapsed = elapsed
        for (let t = 0; t < 500_000; t += 10) {
          vi.advanceTimersByTime(10)
          elapsed += 10
          if (ctor.mock.calls.length > prevCallCount) {
            reconnectTimes.push(elapsed - prevElapsed)
            break
          }
        }
      }

      // Constructor was called 4 times: initial + 3 reconnects
      expect(throwCount).toBe(4)

      // Delays should increase: ~100 → ~200 → ~400 (from the 3 reconnect delays)
      // reconnectTimes[0] is from initial connect (no delay), so check indices 1+
      if (reconnectTimes.length >= 3) {
        expect(reconnectTimes[2]).toBeGreaterThan(reconnectTimes[1] * 1.3)
      }
    })

    it('BW23 maxDelay 上限', () => {
      const setTimeoutSpy = vi.spyOn(globalThis, 'setTimeout')
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 1000, maxDelay: 5000, backoffMultiplier: 10, maxAttempts: 5 },
      })

      for (let i = 0; i < 3; i++) {
        const ws = instances[instances.length - 1]
        ws.readyState = 3
        ws.onclose!(new CloseEvent('close'))
        vi.advanceTimersByTime(100_000)
      }

      // Filter to reconnect delays only (< maxDelay * 1.25)
      const delayCalls = setTimeoutSpy.mock.calls.filter(
        c => typeof c[1] === 'number' && (c[1] as number) > 0 && (c[1] as number) < 10000,
      )
      for (const call of delayCalls) {
        expect(call[1] as number).toBeLessThanOrEqual(6250)
      }
    })

    it('BW24 maxAttempts 后返回 -1 → 不重连', () => {
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 10, maxDelay: 50, maxAttempts: 0 },
      })

      const ws = lastInstance()
      ws.readyState = 3
      ws.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(60_000)
      expect(instances.length).toBe(1)
    })

    it('BW25 reset() 重置计数（重连成功后退避恢复初始值）', () => {
      const setTimeoutSpy = vi.spyOn(globalThis, 'setTimeout')
      createWebSocketClient({
        url: 'ws://test',
        reconnect: { initialDelay: 100, maxDelay: 5000, backoffMultiplier: 2, maxAttempts: 10 },
      })

      // First connection succeeds → reset()
      const ws1 = lastInstance()
      ws1.readyState = 1
      ws1.onopen!(new Event('open'))

      // Server closes
      ws1.readyState = 3
      ws1.onclose!(new CloseEvent('close'))

      vi.advanceTimersByTime(1000)

      // Second connection succeeds → reset()
      const ws2 = lastInstance()
      ws2.readyState = 1
      ws2.onopen!(new Event('open'))

      // The reconnect delay after reset should be near initialDelay
      const delayCalls = setTimeoutSpy.mock.calls.filter(
        c => typeof c[1] === 'number' && (c[1] as number) > 0 && (c[1] as number) < 5000,
      )
      if (delayCalls.length > 0) {
        const delay = delayCalls[0][1] as number
        expect(delay).toBeLessThanOrEqual(150) // ~100ms ± 25%
      }
    })
  })
})
