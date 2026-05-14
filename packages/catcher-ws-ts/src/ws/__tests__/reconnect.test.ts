import { describe, it, expect } from 'vitest'
import { createReconnectStrategy } from '../reconnect.js'

describe('RC1 — First delay ≈ initialDelay', () => {
  it('returns delay within ±25% of initialDelay', () => {
    const strategy = createReconnectStrategy({ initialDelay: 500 })
    const delay = strategy.nextDelay()
    expect(delay).toBeGreaterThanOrEqual(500 * 0.75)
    expect(delay).toBeLessThanOrEqual(500 * 1.25)
  })
})

describe('RC2 — Exponential growth', () => {
  it('delays approximately double each attempt', () => {
    const strategy = createReconnectStrategy({
      initialDelay: 100,
      backoffMultiplier: 2,
      maxDelay: 100_000,
    })
    const delays = [
      strategy.nextDelay(),
      strategy.nextDelay(),
      strategy.nextDelay(),
      strategy.nextDelay(),
    ]

    // delay[0] ≈ 100, delay[1] ≈ 200, delay[2] ≈ 400, delay[3] ≈ 800
    // Allow ±25% jitter
    expect(delays[0]).toBeGreaterThanOrEqual(75)
    expect(delays[0]).toBeLessThanOrEqual(125)

    expect(delays[1]).toBeGreaterThanOrEqual(150)
    expect(delays[1]).toBeLessThanOrEqual(250)

    expect(delays[2]).toBeGreaterThanOrEqual(300)
    expect(delays[2]).toBeLessThanOrEqual(500)

    expect(delays[3]).toBeGreaterThanOrEqual(600)
    expect(delays[3]).toBeLessThanOrEqual(1000)
  })
})

describe('RC3 — maxDelay cap', () => {
  it('delays never exceed maxDelay (with jitter margin)', () => {
    const strategy = createReconnectStrategy({
      initialDelay: 100,
      maxDelay: 300,
      backoffMultiplier: 2,
      maxAttempts: 20,
    })
    for (let i = 0; i < 15; i++) {
      const delay = strategy.nextDelay()
      expect(delay).toBeLessThanOrEqual(300 * 1.25)
    }
  })
})

describe('RC4 — jitter ±25%', () => {
  it('all delays within ±25% of expected base (capped by maxDelay)', () => {
    const strategy = createReconnectStrategy({
      initialDelay: 1000,
      maxDelay: 30_000,
      maxAttempts: 10,
    })
    for (let i = 0; i < 10; i++) {
      const delay = strategy.nextDelay()
      const base = Math.min(1000 * Math.pow(2, i), 30_000)
      expect(delay).toBeGreaterThanOrEqual(Math.floor(base * 0.75))
      expect(delay).toBeLessThanOrEqual(Math.ceil(base * 1.25))
    }
  })
})

describe('RC5 — maxAttempts returns -1', () => {
  it('returns -1 after maxAttempts exceeded', () => {
    const strategy = createReconnectStrategy({ maxAttempts: 3 })
    expect(strategy.nextDelay()).not.toBe(-1) // attempt 1
    expect(strategy.nextDelay()).not.toBe(-1) // attempt 2
    expect(strategy.nextDelay()).not.toBe(-1) // attempt 3
    expect(strategy.nextDelay()).toBe(-1)     // attempt 4 → exceeded
  })
})

describe('RC6 — reset() resets counter', () => {
  it('after reset, delay goes back to initialDelay', () => {
    const strategy = createReconnectStrategy({
      initialDelay: 100,
      maxAttempts: 3,
    })
    strategy.nextDelay() // attempt 1
    strategy.nextDelay() // attempt 2
    strategy.reset()

    expect(strategy.attemptCount).toBe(0)
    const delay = strategy.nextDelay() // should be back to ~100
    expect(delay).toBeGreaterThanOrEqual(75)
    expect(delay).toBeLessThanOrEqual(125)
  })
})

describe('RC7 — attemptCount increments', () => {
  it('counts 1, 2, 3, 4, 5', () => {
    const strategy = createReconnectStrategy({ maxAttempts: 10 })
    expect(strategy.attemptCount).toBe(0)
    strategy.nextDelay()
    expect(strategy.attemptCount).toBe(1)
    strategy.nextDelay()
    expect(strategy.attemptCount).toBe(2)
    strategy.nextDelay()
    expect(strategy.attemptCount).toBe(3)
    strategy.nextDelay()
    expect(strategy.attemptCount).toBe(4)
    strategy.nextDelay()
    expect(strategy.attemptCount).toBe(5)
  })
})

describe('RC8 — Default config is valid', () => {
  it('creates without error and returns positive delay', () => {
    const strategy = createReconnectStrategy()
    const delay = strategy.nextDelay()
    expect(delay).toBeGreaterThan(0)
    expect(typeof delay).toBe('number')
  })
})

describe('RC9 — maxAttempts=0 stops immediately', () => {
  it('first nextDelay() returns -1', () => {
    const strategy = createReconnectStrategy({ maxAttempts: 0 })
    expect(strategy.nextDelay()).toBe(-1)
  })
})
