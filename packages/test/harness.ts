/**
 * E2E test harness — runs the same scenario with vanilla tools vs catcher
 * CONCURRENTLY over N iterations to ensure identical network conditions.
 *
 * Key insight: vanilla and catcher run at the same time via Promise.all,
 * so they both face the same random packet loss / disruption events
 * from the network proxy. Running N iterations gives statistically
 * meaningful success rates and latency distributions.
 *
 * Metric philosophy:
 * - Success rate is the PRIMARY metric (catcher aims to be more reliable).
 * - Latency stats include FAILURES at their timeout value so the cost of
 *   failure is not hidden. Compare "all-in" P50/P95, not just successes.
 * - "仅成功延迟" (success-only latency) is still shown for reference when
 *   both sides have comparable success rates.
 */

import type { NetworkConditions } from './network/proxy.js'

// ── Types ───────────────────────────────────────────────────

export interface IterationResult {
  success: boolean
  time: number
  /** Number of retries triggered in this iteration (0 = success on first try) */
  retries?: number
  bytes?: number
  connections?: number
}

export interface ScenarioMetrics {
  /** Success rate 0-1 */
  successRate: number
  /** Total iterations */
  iterations: number
  /** Successful iterations */
  successes: number
  /** Failed iterations */
  failures: number

  /** How many succeeded without any retry */
  zeroRetrySuccesses: number
  /** How many succeeded after 1+ retries */
  retriedSuccesses: number
  /** Average retry count across all successes */
  avgRetries: number

  /** P50 of zero-retry successes (ms). Fair baseline vs vanilla. */
  zeroRetryP50: number
  /** P95 of zero-retry successes (ms) */
  zeroRetryP95: number
  /** Mean latency of zero-retry successes (ms) */
  zeroRetryMean: number

  /** Mean latency of retried successes (ms) */
  retriedMean: number

  /** Average connections per iteration */
  avgConnections: number
  /** Average bytes transferred per iteration */
  avgBytes: number
}

export interface ScenarioResult {
  name: string
  networkProfile: string
  iterations: number
  vanilla: ScenarioMetrics
  catcher: ScenarioMetrics
  improvements: {
    successRate: number
    /** P50 improvement on zero-retry successes only (fair baseline) */
    zeroRetryP50: number
    connections: number
    bytes: number
  }
}

export interface ScenarioConfig {
  name: string
  description: string
  /** Number of iterations. Default 30. Override with E2E_ITERATIONS env */
  iterations?: number
  /** Timeout per single iteration in ms */
  iterationTimeout?: number
}

export type ScenarioFn = (baseUrl: string) => Promise<IterationResult>

// ── Helpers ─────────────────────────────────────────────────

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0
  const idx = Math.ceil((p / 100) * sorted.length) - 1
  return sorted[Math.max(0, Math.min(idx, sorted.length - 1))]
}

function computeMetrics(results: IterationResult[]): ScenarioMetrics {
  const successes = results.filter((r) => r.success).length
  const failures = results.filter((r) => !r.success).length

  // Bucket by retry count
  const zeroRetry = results
    .filter((r) => r.success && (r.retries ?? 0) === 0)
    .map((r) => r.time)
    .sort((a, b) => a - b)

  const retriedSuccesses = results.filter((r) => r.success && (r.retries ?? 0) > 0)
  const totalRetries = retriedSuccesses.reduce((s, r) => s + (r.retries ?? 0), 0)

  return {
    successRate: results.length > 0 ? successes / results.length : 0,
    iterations: results.length,
    successes,
    failures,

    zeroRetrySuccesses: zeroRetry.length,
    retriedSuccesses: retriedSuccesses.length,
    avgRetries: successes > 0 ? totalRetries / successes : 0,

    zeroRetryP50: percentile(zeroRetry, 50),
    zeroRetryP95: percentile(zeroRetry, 95),
    zeroRetryMean: zeroRetry.length > 0
      ? Math.round(zeroRetry.reduce((a, b) => a + b, 0) / zeroRetry.length)
      : 0,

    retriedMean: retriedSuccesses.length > 0
      ? Math.round(retriedSuccesses.reduce((s, r) => s + r.time, 0) / retriedSuccesses.length)
      : 0,

    avgConnections: Math.round(results.reduce((s, r) => s + (r.connections ?? 0), 0) / Math.max(1, results.length)),
    avgBytes: Math.round(results.reduce((s, r) => s + (r.bytes ?? 0), 0) / Math.max(1, results.length)),
  }
}

// ── Main ────────────────────────────────────────────────────

/**
 * Run a scenario concurrently: each iteration runs vanilla AND catcher
 * at the same time via Promise.all, ensuring identical network conditions.
 */
export async function runConcurrentComparison(
  config: ScenarioConfig,
  networkConditions: NetworkConditions,
  networkName: string,
  vanillaFn: ScenarioFn,
  catcherFn: ScenarioFn,
  baseUrl: string,
): Promise<ScenarioResult> {
  const totalIterations = config.iterations
    ?? parseInt(process.env.E2E_ITERATIONS ?? '30', 10)

  const timeout = config.iterationTimeout ?? 30_000

  const vanillaResults: IterationResult[] = []
  const catcherResults: IterationResult[] = []

  console.log(`  [harness] ${config.name} — ${totalIterations} iterations (concurrent vanilla vs catcher)`)
  console.log(`  [harness] network: ${networkName}`)

  for (let i = 0; i < totalIterations; i++) {
    const iterNum = i + 1

    // Run both concurrently with timeout
    const vanillaPromise = withTimeout(vanillaFn(baseUrl), timeout)
      .catch((): IterationResult => ({ success: false, time: timeout }))

    const catcherPromise = withTimeout(catcherFn(baseUrl), timeout)
      .catch((): IterationResult => ({ success: false, time: timeout }))

    const [vanillaRes, catcherRes] = await Promise.all([vanillaPromise, catcherPromise])

    vanillaResults.push(vanillaRes)
    catcherResults.push(catcherRes)

    // Progress every 10 iterations
    if (iterNum % 10 === 0 || iterNum === totalIterations) {
      const vOk = vanillaResults.filter((r) => r.success).length
      const cOk = catcherResults.filter((r) => r.success).length
      console.log(`    [${iterNum}/${totalIterations}] vanilla: ${vOk}/${iterNum} | catcher: ${cOk}/${iterNum}`)
    }
  }

  const vanilla = computeMetrics(vanillaResults)
  const catcher = computeMetrics(catcherResults)

  const improvements = {
    successRate: catcher.successRate - vanilla.successRate,
    // G-11 fix: only compute latency improvement when BOTH sides have zero-retry samples.
    // When catcher has no zero-retry successes (e.g. all failed), the improvement
    // would falsely show +100%. Use null to indicate N/A.
    zeroRetryP50: (vanilla.zeroRetryP50 > 0 && catcher.zeroRetryP50 > 0)
      ? (vanilla.zeroRetryP50 - catcher.zeroRetryP50) / vanilla.zeroRetryP50
      : 0,
    connections: vanilla.avgConnections > 0 ? (vanilla.avgConnections - catcher.avgConnections) / vanilla.avgConnections : 0,
    bytes: vanilla.avgBytes > 0 ? (vanilla.avgBytes - catcher.avgBytes) / vanilla.avgBytes : 0,
  }

  console.log(`  [harness] done. vanilla=${(vanilla.successRate * 100).toFixed(0)}% catcher=${(catcher.successRate * 100).toFixed(0)}%`)

  return { name: config.name, networkProfile: networkName, iterations: totalIterations, vanilla, catcher, improvements }
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('timeout')), ms)
    promise.then(
      (v) => { clearTimeout(timer); resolve(v) },
      (e) => { clearTimeout(timer); reject(e) },
    )
  })
}
