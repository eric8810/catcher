/**
 * SSE Line Router — routes raw SSE text lines.
 */

export type RouteAction =
  | { kind: 'yield'; line: string }
  | { kind: 'silent' }
  | { kind: 'setLastEventId'; id: string }
  | { kind: 'setRetry'; ms: number }

export function routeLine(line: string): RouteAction {
  if (line === '') return { kind: 'silent' }
  if (line.startsWith(':')) return { kind: 'silent' }
  if (line.startsWith('id:')) {
    return { kind: 'setLastEventId', id: line.slice(3).trimStart() }
  }
  if (line.startsWith('retry:')) {
    const ms = parseInt(line.slice(6).trim(), 10)
    if (Number.isFinite(ms) && ms >= 0) return { kind: 'setRetry', ms }
  }
  return { kind: 'yield', line }
}

/** Async push queue used as bridge between SSE background reader and consumer iterator. */
export class PushQueue<T> {
  private items: T[] = []
  private waiting: Array<(result: IteratorResult<T>) => void> = []
  private _done = false
  private _error: any = null

  push(item: T) {
    if (this._done) return
    if (this.waiting.length > 0) {
      this.waiting.shift()!({ value: item, done: false })
    } else {
      this.items.push(item)
    }
  }

  finish() {
    this._done = true
    for (const resolve of this.waiting) resolve({ value: undefined, done: true })
    this.waiting = []
  }

  fail(error: any) {
    this._error = error
    this.finish()
  }

  get isDone() { return this._done }

  [Symbol.asyncIterator](): AsyncIterator<T> & { return(): Promise<IteratorResult<T>> } {
    return {
      next: (): Promise<IteratorResult<T>> => {
        if (this.items.length > 0) {
          return Promise.resolve({ value: this.items.shift()!, done: false })
        }
        if (this._done) {
          return this._error
            ? Promise.reject(this._error)
            : Promise.resolve({ value: undefined, done: true })
        }
        return new Promise<IteratorResult<T>>(resolve => { this.waiting.push(resolve) })
      },
      return: (): Promise<IteratorResult<T>> => {
        return Promise.resolve({ value: undefined, done: true })
      },
    }
  }
}
