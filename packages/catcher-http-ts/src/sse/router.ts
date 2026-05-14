/**
 * SSE Line Router — routes raw SSE text lines.
 *
 * Silent (consumed by library):
 *   - empty line → event separator
 *   - `:` prefix → comment / heartbeat
 *   - `id:` → record lastEventId
 *   - `retry:` → adjust reconnect interval
 *
 * Yield (passed to user as-is, with prefix):
 *   - `data:`, `event:`, and any other content lines
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
