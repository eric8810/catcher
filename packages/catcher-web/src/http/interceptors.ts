import type {
  InterceptorManager,
  InterceptorHandler,
  InterceptorFulfilled,
  InterceptorRejected,
  RequestConfig,
} from '@eric8810/core'

interface RegisteredHandler<T> {
  id: number
  handler: InterceptorHandler<T>
}

/**
 * Create an interceptor manager with axios-compatible semantics.
 *
 * - `use()` returns a numeric id, handlers execute in registration order.
 * - Request execution is LIFO (last registered = outermost onion layer).
 * - Response execution is FIFO (first registered = innermost).
 * - `eject(id)` removes a handler; `clear()` removes all.
 */
export function createInterceptorManager<T>(): InterceptorManager<T> {
  const handlers: RegisteredHandler<T>[] = []
  let nextId = 1

  const manager: InterceptorManager<T> = {
    use(
      onFulfilled: InterceptorFulfilled<T>,
      onRejected?: InterceptorRejected,
      options?: { runWhen?: (config: RequestConfig) => boolean; synchronous?: boolean },
    ): number {
      const id = nextId++
      handlers.push({
        id,
        handler: {
          onFulfilled,
          onRejected,
          runWhen: options?.runWhen,
          synchronous: options?.synchronous,
        },
      })
      return id
    },

    eject(id: number): void {
      const idx = handlers.findIndex((h) => h.id === id)
      if (idx !== -1) handlers.splice(idx, 1)
    },

    clear(): void {
      handlers.length = 0
    },
  }

  // Attach non-public helpers for running chains.
  // These sit on the manager as hidden properties so client.ts can use them.
  ;(manager as any)._runRequestChain = async function (
    initial: T,
    reqConfig?: RequestConfig,
  ): Promise<T> {
    let value = initial
    // Request: LIFO — last registered runs first (outermost onion layer)
    for (let i = handlers.length - 1; i >= 0; i--) {
      const { handler } = handlers[i]
      if (handler.runWhen && reqConfig && !handler.runWhen(reqConfig)) continue
      try {
        value = await handler.onFulfilled(value)
      } catch (err: any) {
        if (handler.onRejected) {
          value = await handler.onRejected(err)
        } else {
          throw err
        }
      }
    }
    return value
  }

  ;(manager as any)._runResponseChain = async function (initial: T): Promise<T> {
    let value = initial
    // Response: FIFO — first registered runs first
    for (const { handler } of handlers) {
      try {
        value = await handler.onFulfilled(value)
      } catch (err: any) {
        if (handler.onRejected) {
          value = await handler.onRejected(err)
        } else {
          throw err
        }
      }
    }
    return value
  }

  return manager
}
