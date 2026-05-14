import type { SSEStreamOptions, SSEStream, SSETimeoutError } from '@eric8810/catcher-core'
import { routeLine } from './router.js'

class SSETimeoutErrorImpl extends Error implements SSETimeoutError {
  readonly type = 'SSE_TIMEOUT' as const
  constructor(timeout: number) {
    super(`SSE timeout after ${timeout}ms`)
    this.name = 'SSETimeoutError'
  }
}

/**
 * Read one chunk from the ReadableStream with an idle timeout.
 * If no data arrives within `timeoutMs`, throws SSETimeoutError.
 */
function readWithIdleTimeout(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<{ done: boolean; value?: Uint8Array }> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error('Aborted'))
      return
    }

    const timer = setTimeout(() => reject(new SSETimeoutErrorImpl(timeoutMs)), timeoutMs)

    const onAbort = () => {
      clearTimeout(timer)
      reject(new Error('Aborted'))
    }
    signal?.addEventListener('abort', onAbort, { once: true })

    reader.read().then(
      result => {
        clearTimeout(timer)
        signal?.removeEventListener('abort', onAbort)
        resolve(result)
      },
      error => {
        clearTimeout(timer)
        signal?.removeEventListener('abort', onAbort)
        reject(error)
      },
    )
  })
}

/**
 * Create a one-shot SSE stream.
 *
 * Connects via fetch, reads the response body as a ReadableStream,
 * buffers by `\n`, routes lines, and yields content lines.
 * No auto-reconnect — when the stream ends, iteration ends.
 */
export function createSSEStream(options: SSEStreamOptions): SSEStream {
  let lastEventId = ''
  let reconnectDelay = 0
  let iterated = false

  const stream: SSEStream = {
    get lastEventId() { return lastEventId },

    [Symbol.asyncIterator]() {
      if (iterated) throw new Error('SSEStream can only be iterated once')
      iterated = true

      return (async function* () {
        const {
          url,
          method = 'GET',
          headers: baseHeaders = {},
          body,
          timeout = 30_000,
          signal,
        } = options

        // Build request headers
        const headers: Record<string, string> = { ...baseHeaders }
        if (body !== undefined && !headers['Content-Type'] && !headers['content-type']) {
          headers['Content-Type'] = 'application/json'
        }

        const init: RequestInit = {
          method,
          headers,
          body: body !== undefined
            ? (typeof body === 'string' ? body : JSON.stringify(body))
            : undefined,
        }

        // Combine timeout + user signal via AbortController
        const controller = new AbortController()
        const timeoutId = setTimeout(() => controller.abort(), timeout)
        const onUserAbort = () => controller.abort()
        signal?.addEventListener('abort', onUserAbort, { once: true })
        init.signal = controller.signal

        let response: Response
        try {
          response = await fetch(url, init)
        } catch (err: any) {
          if (signal?.aborted) throw err
          if (err.name === 'AbortError') throw new SSETimeoutErrorImpl(timeout)
          throw err
        } finally {
          clearTimeout(timeoutId)
          signal?.removeEventListener('abort', onUserAbort)
        }

        if (!response.ok) {
          throw new Error(`SSE connection failed: HTTP ${response.status}`)
        }
        if (!response.body) {
          throw new Error('SSE: response body is null (ReadableStream not available)')
        }

        // Read stream → chunk buffer → route lines → yield content
        const reader = response.body.getReader()
        const decoder = new TextDecoder()
        let buffer = ''

        try {
          while (true) {
            const { done, value } = await readWithIdleTimeout(reader, timeout, signal)
            if (done) break

            buffer += decoder.decode(value, { stream: true })

            let newlineIdx: number
            while ((newlineIdx = buffer.indexOf('\n')) !== -1) {
              let line = buffer.slice(0, newlineIdx)
              buffer = buffer.slice(newlineIdx + 1)
              if (line.endsWith('\r')) line = line.slice(0, -1)

              const action = routeLine(line)
              switch (action.kind) {
                case 'yield': yield action.line; break
                case 'setLastEventId': lastEventId = action.id; break
                case 'setRetry': reconnectDelay = action.ms; break
                case 'silent': break
              }
            }
          }

          // Process remaining buffer (last line without trailing \n)
          if (buffer.length > 0) {
            let line = buffer
            if (line.endsWith('\r')) line = line.slice(0, -1)
            const action = routeLine(line)
            if (action.kind === 'yield') yield action.line
            else if (action.kind === 'setLastEventId') lastEventId = action.id
            else if (action.kind === 'setRetry') reconnectDelay = action.ms
          }
        } finally {
          reader.releaseLock()
        }
      })()
    },
  }

  return stream
}
