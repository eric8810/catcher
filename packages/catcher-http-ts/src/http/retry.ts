import pRetry, { AbortError } from 'p-retry'
import type { AxiosInstance } from 'axios'
import type { Socket } from 'node:net'
import type { RetryOptions } from '@eric8810/core'

/**
 * Wrap an axios instance with p-retry.
 *
 * Fixes (Issues #1, #3):
 * - On retry, destroys idle keepAlive sockets to force a fresh connection (#1)
 * - Retries on ECONNRESET, ETIMEDOUT, ENOTFOUND, ECONNREFUSED, and 5xx (#3)
 */
export function createRetryWrapper(
  instance: AxiosInstance,
  options: RetryOptions,
) {
  const { attempts, backoff = 'exponential', minTimeout, maxTimeout } = options

  let retriesUsed = 0

  /** Destroy idle free sockets so retries use a fresh connection (#1) */
  const destroyFreeSockets = () => {
    const agent = (instance.defaults as any).httpsAgent
    if (!agent) return
    const freeSockets = agent.freeSockets ?? (agent as any)._freeSockets
    if (!freeSockets) return
    for (const sockets of Object.values(freeSockets) as Socket[][]) {
      for (const socket of sockets) {
        try { socket.destroy() } catch { /* mute */ }
      }
    }
  }

  return async (method: string, ...args: any[]) => {
    return pRetry(
      async (attemptNum) => {
        // On retry attempts, evict stale connections from the pool (#1)
        if (attemptNum > 1) {
          destroyFreeSockets()
        }

        try {
          return await (instance as any)[method](...args)
        } catch (error: any) {
          // Retry on network errors or 5xx.
          // ETIMEDOUT is included because in weak networks it's often caused
          // by packet loss/delays, not server overload. The circuit breaker
          // (#4) prevents retry storms when failures are persistent. (#3)
          const isRetryable =
            error.code === 'ECONNRESET' ||
            error.code === 'ETIMEDOUT' ||
            error.code === 'ENOTFOUND' ||
            error.code === 'ECONNREFUSED' ||
            (error.response?.status ?? 0) >= 500

          if (isRetryable) {
            throw error // p-retry will catch and retry
          }
          // Don't retry 4xx, ETIMEDOUT, etc — mark as non-retryable
          throw new AbortError(error)
        }
      },
      {
        retries: attempts,
        factor: backoff === 'exponential' ? 2 : 1,
        minTimeout: minTimeout ?? 500,
        maxTimeout: maxTimeout ?? 30_000,
        onFailedAttempt: (error) => {
          retriesUsed++
          options.onRetry?.(error.attemptNumber)
          console.warn(
            `[catcher] Attempt ${error.attemptNumber}/${attempts + 1} failed: ${error.message}`
          )
        },
      },
    )
  }
}
