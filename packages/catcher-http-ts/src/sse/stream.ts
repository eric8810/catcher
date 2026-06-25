import type { SSEStreamOptions, SSEStream } from '@eric8810/catcher-core'
import { createSSEStreamCore, SSETimeoutErrorImpl } from '@eric8810/catcher-core'

function readWithIdleTimeout(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<{ done: boolean; value?: Uint8Array }> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(new Error('Aborted')); return }
    const timer = setTimeout(() => reject(new SSETimeoutErrorImpl(timeoutMs)), timeoutMs)
    const onAbort = () => { clearTimeout(timer); reject(new Error('Aborted')) }
    signal?.addEventListener('abort', onAbort, { once: true })
    reader.read().then(
      result => { clearTimeout(timer); signal?.removeEventListener('abort', onAbort); resolve(result) },
      error => { clearTimeout(timer); signal?.removeEventListener('abort', onAbort); reject(error) },
    )
  })
}

export function createSSEStream(options: SSEStreamOptions): SSEStream {
  return createSSEStreamCore(options, readWithIdleTimeout)
}
