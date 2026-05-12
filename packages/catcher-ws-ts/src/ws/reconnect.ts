interface ReconnectStrategy {
  nextDelay: () => number
  reset: () => void
}

export function createReconnectStrategy(opts?: {
  initialDelay?: number
  maxDelay?: number
  backoffMultiplier?: number
  maxAttempts?: number
}): ReconnectStrategy {
  const {
    initialDelay = 1000,
    maxDelay = 30_000,
    backoffMultiplier = 2,
    maxAttempts = 20,
  } = opts ?? {}

  let attempt = 0

  return {
    nextDelay(): number {
      attempt++
      const exponential = initialDelay * Math.pow(backoffMultiplier, Math.max(0, attempt - 1))
      const delay = Math.min(exponential, maxDelay)
      // Add jitter: ±25%
      const jitter = (Math.random() - 0.5) * 0.5 * delay
      return Math.round(delay + jitter)
    },
    reset(): void {
      attempt = 0
    },
  }
}
