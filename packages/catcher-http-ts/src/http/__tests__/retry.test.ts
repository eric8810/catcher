import { describe, it, expect, vi, beforeEach } from 'vitest'
import axios from 'axios'
import { createRetryWrapper } from '../retry.js'

function createMockInstance() {
  const instance = axios.create()
  const get = vi.fn()
  ;(instance as any).get = get
  return { instance, get }
}

function networkError(code: string) {
  const err: any = new Error(code)
  err.code = code
  return err
}

function httpError(status: number) {
  const err: any = new Error(`HTTP ${status}`)
  err.response = { status, data: null }
  return err
}

// ── R1-R6: Retry trigger conditions ──────────────────────────────

describe('R1 — ECONNRESET auto-retry', () => {
  it('retries on ECONNRESET and succeeds on 3rd attempt', async () => {
    const { instance, get } = createMockInstance()
    get
      .mockRejectedValueOnce(networkError('ECONNRESET'))
      .mockRejectedValueOnce(networkError('ECONNRESET'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 3, minTimeout: 1 })
    const result = await wrap('get', '/test')

    expect(get).toHaveBeenCalledTimes(3)
    expect(result.status).toBe(200)
  })
})

describe('R2 — 5xx auto-retry', () => {
  it('retries on 503 and succeeds on 2nd attempt', async () => {
    const { instance, get } = createMockInstance()
    get
      .mockRejectedValueOnce(httpError(503))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    const result = await wrap('get', '/test')

    expect(get).toHaveBeenCalledTimes(2)
    expect(result.status).toBe(200)
  })
})

describe('R3 — 4xx no retry', () => {
  it('does not retry on 403 and throws immediately', async () => {
    const { instance, get } = createMockInstance()
    get.mockRejectedValue(httpError(403))

    const wrap = createRetryWrapper(instance, { attempts: 3, minTimeout: 1 })
    await expect(wrap('get', '/test')).rejects.toThrow('HTTP 403')
    expect(get).toHaveBeenCalledTimes(1)
  })
})

describe('R4 — ETIMEDOUT retry', () => {
  it('retries on ETIMEDOUT', async () => {
    const { instance, get } = createMockInstance()
    get
      .mockRejectedValueOnce(networkError('ETIMEDOUT'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    const result = await wrap('get', '/test')
    expect(result.status).toBe(200)
  })
})

describe('R5 — ENOTFOUND retry', () => {
  it('retries on ENOTFOUND', async () => {
    const { instance, get } = createMockInstance()
    get
      .mockRejectedValueOnce(networkError('ENOTFOUND'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    const result = await wrap('get', '/test')
    expect(result.status).toBe(200)
  })
})

describe('R6 — ECONNREFUSED retry', () => {
  it('retries on ECONNREFUSED', async () => {
    const { instance, get } = createMockInstance()
    get
      .mockRejectedValueOnce(networkError('ECONNREFUSED'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    const result = await wrap('get', '/test')
    expect(result.status).toBe(200)
  })
})

// ── R7-R9: Backoff strategies ────────────────────────────────────

describe('R7 — Exponential backoff', () => {
  it('delays approximately double each attempt', async () => {
    const { instance, get } = createMockInstance()
    const timestamps: number[] = []
    get.mockImplementation(async () => {
      timestamps.push(Date.now())
      if (timestamps.length < 4) throw networkError('ECONNRESET')
      return { status: 200, data: 'ok' }
    })

    const wrap = createRetryWrapper(instance, {
      attempts: 4,
      backoff: 'exponential',
      minTimeout: 50,
    })
    await wrap('get', '/test')

    // With factor=2 and minTimeout=50: delays ~50, ~100, ~200
    const gap1 = timestamps[1] - timestamps[0]
    const gap2 = timestamps[2] - timestamps[1]
    const gap3 = timestamps[3] - timestamps[2]

    expect(gap1).toBeGreaterThanOrEqual(30) // ~50ms
    expect(gap2).toBeGreaterThanOrEqual(gap1 * 1.2) // ~100ms, allow jitter
    expect(gap3).toBeGreaterThanOrEqual(gap2 * 1.2) // ~200ms
  })
})

describe('R8 — Constant backoff', () => {
  it('delays are approximately equal', async () => {
    const { instance, get } = createMockInstance()
    const timestamps: number[] = []
    get.mockImplementation(async () => {
      timestamps.push(Date.now())
      if (timestamps.length < 3) throw networkError('ECONNRESET')
      return { status: 200, data: 'ok' }
    })

    const wrap = createRetryWrapper(instance, {
      attempts: 3,
      backoff: 'fixed',
      minTimeout: 50,
    })
    await wrap('get', '/test')

    const gap1 = timestamps[1] - timestamps[0]
    const gap2 = timestamps[2] - timestamps[1]
    // Both gaps should be ~50ms (within 30ms tolerance)
    expect(Math.abs(gap1 - gap2)).toBeLessThan(30)
  })
})

describe('R9 — maxTimeout cap', () => {
  it('backoff does not exceed maxTimeout', async () => {
    const { instance, get } = createMockInstance()
    const timestamps: number[] = []
    get.mockImplementation(async () => {
      timestamps.push(Date.now())
      if (timestamps.length < 4) throw networkError('ECONNRESET')
      return { status: 200, data: 'ok' }
    })

    const wrap = createRetryWrapper(instance, {
      attempts: 4,
      backoff: 'exponential',
      minTimeout: 50,
      maxTimeout: 80,
    })
    await wrap('get', '/test')

    for (let i = 1; i < timestamps.length; i++) {
      const gap = timestamps[i] - timestamps[i - 1]
      expect(gap).toBeLessThanOrEqual(120) // 80ms + margin
    }
  })
})

// ── R10-R13: Callbacks and boundaries ────────────────────────────

describe('R10 — onRetry callback', () => {
  it('calls onRetry for each failed attempt', async () => {
    const { instance, get } = createMockInstance()
    const onRetry = vi.fn()
    get
      .mockRejectedValueOnce(networkError('ECONNRESET'))
      .mockRejectedValueOnce(networkError('ECONNRESET'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, {
      attempts: 3,
      minTimeout: 1,
      onRetry,
    })
    await wrap('get', '/test')

    expect(onRetry).toHaveBeenCalledTimes(2)
    expect(onRetry).toHaveBeenCalledWith(1)
    expect(onRetry).toHaveBeenCalledWith(2)
  })
})

describe('R11 — Max attempts exceeded throws', () => {
  it('throws after all attempts exhausted', async () => {
    const { instance, get } = createMockInstance()
    get.mockRejectedValue(networkError('ECONNRESET'))

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    await expect(wrap('get', '/test')).rejects.toThrow('ECONNRESET')
    // attempts=2 means 3 total tries (initial + 2 retries)
    expect(get).toHaveBeenCalledTimes(3)
  })
})

describe('R12 — First success no retry', () => {
  it('does not retry if first attempt succeeds', async () => {
    const { instance, get } = createMockInstance()
    const onRetry = vi.fn()
    get.mockResolvedValue({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, {
      attempts: 3,
      minTimeout: 1,
      onRetry,
    })
    await wrap('get', '/test')

    expect(get).toHaveBeenCalledTimes(1)
    expect(onRetry).not.toHaveBeenCalled()
  })
})

describe('R13 — destroyFreeSockets on retry', () => {
  it('destroys free sockets on retry attempt', async () => {
    const { instance, get } = createMockInstance()
    const destroy = vi.fn()
    ;(instance.defaults as any).httpsAgent = {
      freeSockets: { 'localhost:443::': [{ destroy }] },
    }
    get
      .mockRejectedValueOnce(networkError('ECONNRESET'))
      .mockResolvedValueOnce({ status: 200, data: 'ok' })

    const wrap = createRetryWrapper(instance, { attempts: 2, minTimeout: 1 })
    await wrap('get', '/test')

    expect(destroy).toHaveBeenCalled()
  })
})
