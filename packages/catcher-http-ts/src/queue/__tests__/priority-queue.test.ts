import { describe, it, expect } from 'vitest'
import { createPriorityQueue, enqueueWithPriority } from '../priority-queue.js'

describe('Q1 — Basic enqueue and result', () => {
  it('add() resolves with task result', async () => {
    const queue = createPriorityQueue()
    const result = await queue.add(async () => 42)
    expect(result).toBe(42)
  })
})

describe('Q2 — Priority ordering', () => {
  it('higher priority (higher number) is dequeued first when multiple tasks are pending', async () => {
    const queue = createPriorityQueue({ concurrency: 1 })
    const order: string[] = []

    // Block the queue with a long task
    const blocker = queue.add(async () => {
      await new Promise((r) => setTimeout(r, 80))
    })

    // Wait for blocker to start running
    await new Promise((r) => setTimeout(r, 10))

    // Add low priority then high priority while blocker is running
    // p-queue: higher number = higher priority = dequeued first
    queue.add(async () => { order.push('low') }, { priority: 0 })
    queue.add(async () => { order.push('high') }, { priority: 10 })

    await blocker
    await queue.onIdle()

    expect(order).toEqual(['high', 'low'])
  })
})

describe('Q3 — Concurrency limit', () => {
  it('never exceeds concurrency limit', async () => {
    const concurrency = 2
    const queue = createPriorityQueue({ concurrency })
    let running = 0
    let maxRunning = 0

    const tasks = Array.from({ length: 10 }, () =>
      queue.add(async () => {
        running++
        maxRunning = Math.max(maxRunning, running)
        await new Promise((r) => setTimeout(r, 20))
        running--
      }),
    )

    await Promise.all(tasks)
    expect(maxRunning).toBeLessThanOrEqual(concurrency)
  })
})

describe('Q4 — Timeout', () => {
  it('rejects task that exceeds timeout', async () => {
    const queue = createPriorityQueue({ concurrency: 1, timeout: 50 })
    await expect(
      queue.add(async () => {
        await new Promise((r) => setTimeout(r, 200))
      }),
    ).rejects.toThrow()
  })
})

describe('Q5 — enqueueWithPriority', () => {
  it('equivalent to queue.add(fn, { priority })', async () => {
    const queue = createPriorityQueue({ concurrency: 1 })
    const order: string[] = []

    // Block the queue
    const blocker = queue.add(async () => {
      await new Promise((r) => setTimeout(r, 80))
    })
    await new Promise((r) => setTimeout(r, 10))

    // Use enqueueWithPriority for low, queue.add for high
    // p-queue: higher number = higher priority
    enqueueWithPriority(queue, 0, async () => { order.push('low') })
    queue.add(async () => { order.push('high') }, { priority: 10 })

    await blocker
    await queue.onIdle()

    expect(order).toEqual(['high', 'low'])
  })
})
