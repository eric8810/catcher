import { describe, it, expect } from 'vitest'
import { createInterceptorManager } from '../interceptors.js'

// ── I1-I6: Registration and execution ────────────────────────────

describe('I1 — use() registers and executes handler', () => {
  it('handler is called and return value passed through', async () => {
    const mgr = createInterceptorManager<any>()
    mgr.use(async (val) => ({ ...val, x: 1 }))

    const result = await (mgr as any)._runRequestChain({})
    expect(result).toEqual({ x: 1 })
  })
})

describe('I2 — Request chain LIFO (last registered runs first)', () => {
  it('handlers execute in reverse registration order', async () => {
    const mgr = createInterceptorManager<any>()
    const order: string[] = []

    mgr.use(async (val) => { order.push('A'); return { ...val, a: 1 } })
    mgr.use(async (val) => { order.push('B'); return { ...val, b: 2 } })
    mgr.use(async (val) => { order.push('C'); return { ...val, c: 3 } })

    const result = await (mgr as any)._runRequestChain({})

    // LIFO: C first, then B, then A
    expect(order).toEqual(['C', 'B', 'A'])
    expect(result).toEqual({ a: 1, b: 2, c: 3 })
  })
})

describe('I3 — Response chain FIFO (first registered runs first)', () => {
  it('handlers execute in registration order', async () => {
    const mgr = createInterceptorManager<any>()
    const order: string[] = []

    mgr.use(async (val) => { order.push('A'); return { ...val, a: 1 } })
    mgr.use(async (val) => { order.push('B'); return { ...val, b: 2 } })
    mgr.use(async (val) => { order.push('C'); return { ...val, c: 3 } })

    const result = await (mgr as any)._runResponseChain({})

    // FIFO: A, B, C
    expect(order).toEqual(['A', 'B', 'C'])
    expect(result).toEqual({ a: 1, b: 2, c: 3 })
  })
})

describe('I4 — eject() removes handler', () => {
  it('ejected handler is no longer called', async () => {
    const mgr = createInterceptorManager<any>()
    const id = mgr.use(async (val) => ({ ...val, x: 1 }))
    mgr.eject(id)

    const result = await (mgr as any)._runRequestChain({})
    expect(result).toEqual({})
  })
})

describe('I5 — clear() removes all handlers', () => {
  it('no handlers run after clear()', async () => {
    const mgr = createInterceptorManager<any>()
    mgr.use(async (val) => ({ ...val, a: 1 }))
    mgr.use(async (val) => ({ ...val, b: 2 }))
    mgr.use(async (val) => ({ ...val, c: 3 }))
    mgr.clear()

    const result = await (mgr as any)._runRequestChain({})
    expect(result).toEqual({})
  })
})

describe('I6 — use() returns incrementing IDs', () => {
  it('IDs are 1, 2, 3', () => {
    const mgr = createInterceptorManager<any>()
    const id1 = mgr.use(async (v) => v)
    const id2 = mgr.use(async (v) => v)
    const id3 = mgr.use(async (v) => v)

    expect(id1).toBe(1)
    expect(id2).toBe(2)
    expect(id3).toBe(3)
  })
})

// ── I7-I9: Error handling ────────────────────────────────────────

describe('I7 — onRejected catches error', () => {
  it('onRejected recovers from thrown error', async () => {
    const mgr = createInterceptorManager<any>()
    mgr.use(
      async () => { throw new Error('boom') },
      async (err) => ({ recovered: true, message: err.message }),
    )

    const result = await (mgr as any)._runRequestChain({})
    expect(result).toEqual({ recovered: true, message: 'boom' })
  })
})

describe('I8 — No onRejected, error propagates', () => {
  it('error propagates when no onRejected handler', async () => {
    const mgr = createInterceptorManager<any>()
    mgr.use(async () => { throw new Error('boom') })

    await expect((mgr as any)._runRequestChain({})).rejects.toThrow('boom')
  })
})

describe('I9 — runWhen condition filter', () => {
  it('skips handler when runWhen returns false', async () => {
    const mgr = createInterceptorManager<any>()
    mgr.use(
      async (val) => ({ ...val, tagged: true }),
      undefined,
      { runWhen: (config: any) => config.dryRun === true },
    )

    // runWhen returns false — handler skipped
    const result1 = await (mgr as any)._runRequestChain({}, { dryRun: false })
    expect(result1).toEqual({})

    // runWhen returns true — handler runs
    const result2 = await (mgr as any)._runRequestChain({}, { dryRun: true })
    expect(result2).toEqual({ tagged: true })
  })
})
