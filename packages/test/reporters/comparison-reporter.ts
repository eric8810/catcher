/**
 * Comparison reporter — generates JSON data + Markdown report
 * for concurrent N-iteration E2E comparisons.
 *
 * Metric philosophy:
 * - Success rate is PRIMARY.
 * - Latency is compared on 0-retry successes ONLY (fair baseline).
 * - Retried successes are shown separately as "retry cost".
 * - Failures are counted in success rate, not latency.
 */

import fs from 'node:fs/promises'
import path from 'node:path'
import type { ScenarioResult } from '../harness.js'

function pct(v: number): string {
  const sign = v >= 0 ? '+' : '-'
  return `${sign}${Math.abs(v * 100).toFixed(1)}%`
}

function ms(v: number): string {
  if (v === 0) return '-'
  if (v >= 1000) return `${(v / 1000).toFixed(1)}s`
  if (v < 1) return `${(v * 1000).toFixed(0)}μs`
  return `${Math.round(v)}ms`
}

function rateStr(successes: number, total: number): string {
  const p = total > 0 ? (successes / total) * 100 : 0
  return `${p.toFixed(0)}% (${successes}/${total})`
}

function fmtBytes(v: number): string {
  if (v === 0) return '0B'
  if (v >= 1024) return `${(v / 1024).toFixed(1)}KB`
  return `${Math.round(v)}B`
}

export class ComparisonReporter {
  private results: ScenarioResult[] = []

  addResult(result: ScenarioResult): void {
    this.results.push(result)
  }

  private buildSummary(): object {
    if (this.results.length === 0) return {}

    const avgImprovement = {
      successRate: this.results.reduce((s, r) => s + r.improvements.successRate, 0) / this.results.length,
      zeroRetryP50: this.results
        .filter(r => r.vanilla.zeroRetryP50 > 0 && r.catcher.zeroRetryP50 > 0)
        .reduce((s, r) => s + r.improvements.zeroRetryP50, 0)
        / Math.max(1, this.results.filter(r => r.vanilla.zeroRetryP50 > 0 && r.catcher.zeroRetryP50 > 0).length),
      // G-11 fix: show absolute P50 diff alongside relative percentage for small-baseline scenarios
      zeroRetryP50AbsDiff: this.results
        .filter(r => r.vanilla.zeroRetryP50 > 0 && r.catcher.zeroRetryP50 > 0)
        .reduce((s, r) => s + (r.vanilla.zeroRetryP50 - r.catcher.zeroRetryP50), 0)
        / Math.max(1, this.results.filter(r => r.vanilla.zeroRetryP50 > 0 && r.catcher.zeroRetryP50 > 0).length),
    }

    // Count scenarios with retry success
    const scenariosWithRetry = this.results.filter(r => r.catcher.retriedSuccesses > 0)
    const avgRetryRate = scenariosWithRetry.length > 0
      ? scenariosWithRetry.reduce((s, r) => s + r.catcher.retriedSuccesses / Math.max(1, r.catcher.successes), 0) / scenariosWithRetry.length
      : 0
    const avgRetryPenalty = scenariosWithRetry.length > 0
      ? scenariosWithRetry
          .filter(r => r.catcher.retriedMean > 0 && r.catcher.zeroRetryMean > 0)
          .reduce((s, r) => s + (r.catcher.retriedMean - r.catcher.zeroRetryMean), 0)
        / Math.max(1, scenariosWithRetry.filter(r => r.catcher.retriedMean > 0 && r.catcher.zeroRetryMean > 0).length)
      : 0

    return {
      totalScenarios: this.results.length,
      averageImprovement: {
        successRate: pct(avgImprovement.successRate),
        zeroRetryP50: pct(avgImprovement.zeroRetryP50),
        zeroRetryP50AbsDiff: ms(avgImprovement.zeroRetryP50AbsDiff),
      },
      retry: {
        scenariosWithRetry: scenariosWithRetry.length,
        avgRetryRate: `${(avgRetryRate * 100).toFixed(0)}% of successes`,
        avgRetryPenalty: ms(avgRetryPenalty),
      },
    }
  }

  toMarkdown(): string {
    const lines: string[] = []

    lines.push('# Catcher 端到端性能对比测试报告')
    lines.push('')
    lines.push(`> 生成时间: ${new Date().toISOString()}`)
    lines.push(`> 场景数: ${this.results.length}`)
    lines.push(`> 对比方式: 每个场景 vanilla 与 catcher **同网络条件并发** 跑 N 轮`)
    lines.push('> ')
    lines.push('> **延迟按重试次数分桶**：仅 0-retry 成功参与公平延迟对比，retry 成功单独展示代价。')
    lines.push('')

    if (this.results.length > 0) {
      // ═══ Summary table ═══
      lines.push('## 汇总 — 成功率 + 0-retry 延迟（公平基线）')
      lines.push('')
      lines.push('| 场景 | 网络 | Vanilla | Catcher | 成功率改善 | 0-retry P50 Vanilla | 0-retry P50 Catcher | P50改善 | P50绝对差 | Catcher重试率 | 重试代价 |')
      lines.push('|------|------|---------|---------|---------|--------------------|--------------------|--------|----------|-------------|---------|')

      for (const r of this.results) {
        const hasBothZero = r.vanilla.zeroRetryP50 > 0 && r.catcher.zeroRetryP50 > 0
        const p50Imp = hasBothZero ? pct(r.improvements.zeroRetryP50) : 'N/A'
        const p50Abs = hasBothZero ? ms(r.vanilla.zeroRetryP50 - r.catcher.zeroRetryP50) : 'N/A'
        const retryRate = r.catcher.successes > 0
          ? `${((r.catcher.retriedSuccesses / r.catcher.successes) * 100).toFixed(0)}% (${r.catcher.retriedSuccesses}/${r.catcher.successes})`
          : '-'
        const retryPenalty = r.catcher.retriedMean > 0 && r.catcher.zeroRetryMean > 0
          ? `+${ms(r.catcher.retriedMean - r.catcher.zeroRetryMean)}`
          : '-'

        lines.push(
          `| ${r.name} | ${r.networkProfile} | ${rateStr(r.vanilla.successes, r.vanilla.iterations)} | ${rateStr(r.catcher.successes, r.catcher.iterations)} | ${pct(r.improvements.successRate)} | ${r.vanilla.zeroRetryP50 > 0 ? ms(r.vanilla.zeroRetryP50) : '-'} | ${r.catcher.zeroRetryP50 > 0 ? ms(r.catcher.zeroRetryP50) : '-'} | ${p50Imp} | ${p50Abs} | ${retryRate} | ${retryPenalty} |`,
        )
      }

      lines.push('')

      // ═══ Per-scenario details ═══
      for (const r of this.results) {
        lines.push(`## ${r.name} — ${r.networkProfile}`)
        lines.push('')
        lines.push(`> ${r.iterations} 次并发对比，每次 vanilla 和 catcher 同时跑`)

        // ── Success rate ──
        lines.push('')
        lines.push('### 成功率')
        lines.push('')
        lines.push('| | Vanilla | Catcher | 改善 |')
        lines.push('|------|--------|---------|------|')
        lines.push(`| 成功 | ${r.vanilla.successes} | ${r.catcher.successes} | ${pct(r.improvements.successRate)} |`)
        lines.push(`| 失败 | ${r.vanilla.failures} | ${r.catcher.failures} | — |`)
        lines.push(`| 成功率 | ${(r.vanilla.successRate * 100).toFixed(1)}% | ${(r.catcher.successRate * 100).toFixed(1)}% | — |`)

        // ── 0-retry latency (fair baseline) ──
        const vZero = r.vanilla.zeroRetryP50 > 0
        const cZero = r.catcher.zeroRetryP50 > 0
        if (vZero || cZero) {
          lines.push('')
          lines.push('### 0-retry 延迟（公平基线 — 双方均未触发重试的请求）')
          lines.push('')
          lines.push('| | Vanilla | Catcher |')
          lines.push('|------|--------|---------|')
          lines.push(`| 样本数 | ${r.vanilla.zeroRetrySuccesses}/${r.vanilla.iterations} | ${r.catcher.zeroRetrySuccesses}/${r.catcher.iterations} |`)
          lines.push(`| P50 | ${vZero ? ms(r.vanilla.zeroRetryP50) : '-'} | ${cZero ? ms(r.catcher.zeroRetryP50) : '-'} |`)
          lines.push(`| P95 | ${r.vanilla.zeroRetryP95 > 0 ? ms(r.vanilla.zeroRetryP95) : '-'} | ${r.catcher.zeroRetryP95 > 0 ? ms(r.catcher.zeroRetryP95) : '-'} |`)
          lines.push(`| 均值 | ${r.vanilla.zeroRetryMean > 0 ? ms(r.vanilla.zeroRetryMean) : '-'} | ${r.catcher.zeroRetryMean > 0 ? ms(r.catcher.zeroRetryMean) : '-'} |`)
        }

        // ── Retry cost ──
        if (r.catcher.retriedSuccesses > 0) {
          lines.push('')
          lines.push('### 重试成本（catcher 独有）')
          lines.push('')
          const retryPenalty = r.catcher.retriedMean > 0 && r.catcher.zeroRetryMean > 0
            ? (r.catcher.retriedMean - r.catcher.zeroRetryMean)
            : 0
          lines.push(`| 指标 | 值 |`)
          lines.push(`|------|-----|`)
          lines.push(`| 触发重试的成功请求 | ${r.catcher.retriedSuccesses}/${r.catcher.successes} (${((r.catcher.retriedSuccesses / r.catcher.successes) * 100).toFixed(0)}%) |`)
          lines.push(`| 平均重试次数 | ${r.catcher.avgRetries.toFixed(1)} |`)
          lines.push(`| 重试成功平均延迟 | ${ms(r.catcher.retriedMean)} |`)
          lines.push(`| 相比 0-retry 多花 | +${ms(retryPenalty)} |`)
        }

        // ── Connections / Bytes ──
        if (r.vanilla.avgConnections > 0 || r.catcher.avgConnections > 0) {
          lines.push('')
          lines.push(`| 平均连接数 | ${r.vanilla.avgConnections} | ${r.catcher.avgConnections} | ${pct(r.improvements.connections)} |`)
        }
        if (r.catcher.avgBytes > 0) {
          // S5: catcher.avgBytes = jsonSize - msgpackSize (positive = msgpackr saved)
          if (r.vanilla.avgBytes === 0) {
            lines.push(`| msgpackr 带宽节省 | — | **${fmtBytes(r.catcher.avgBytes)}/请求** | — |`)
          } else if (r.vanilla.avgBytes > 0) {
            lines.push(`| 平均传输 | ${fmtBytes(r.vanilla.avgBytes)} | ${fmtBytes(r.catcher.avgBytes)} | ${pct(r.improvements.bytes)} |`)
          }
        } else if (r.vanilla.avgBytes > 0) {
          lines.push(`| 平均传输 | ${fmtBytes(r.vanilla.avgBytes)} | ${r.catcher.avgBytes}B | — |`)
        }

        lines.push('')
      }

      const summary = this.buildSummary() as any
      lines.push('## 总体改善')
      lines.push('')
      lines.push('> **成功率改善**是主指标（catcher 目标是更可靠）。')
      lines.push('> **0-retry P50 改善**仅在双方都有无重试成功的场景中平均，公平对比基础设施开销。')
      lines.push('')
      lines.push(`- 平均成功率改善: **${summary.averageImprovement?.successRate}**`)
      lines.push(`- 平均 0-retry P50 改善: **${summary.averageImprovement?.zeroRetryP50}** (绝对差值: ${summary.averageImprovement?.zeroRetryP50AbsDiff}) (${summary.totalScenarios} 场景)`)
      if (summary.retry?.scenariosWithRetry > 0) {
        lines.push('')
        lines.push('### 重试统计')
        lines.push('')
        lines.push(`- ${summary.retry.scenariosWithRetry} 个场景中 catcher 触发了重试`)
        lines.push(`- 平均重试率: ${summary.retry.avgRetryRate}`)
        lines.push(`- 重试平均额外延迟: **${summary.retry.avgRetryPenalty}**`)
      }
      lines.push('')
    }

    return lines.join('\n')
  }

  async writeReports(outputDir: string): Promise<void> {
    await fs.mkdir(outputDir, { recursive: true })

    const jsonPath = path.join(outputDir, 'comparison-results.json')
    await fs.writeFile(jsonPath, JSON.stringify({
      generatedAt: new Date().toISOString(),
      summary: this.buildSummary(),
      scenarios: this.results.map((r) => ({
        name: r.name,
        network: r.networkProfile,
        iterations: r.iterations,
        vanilla: {
          successRate: r.vanilla.successRate,
          successes: r.vanilla.successes,
          failures: r.vanilla.failures,
          zeroRetrySuccesses: r.vanilla.zeroRetrySuccesses,
          zeroRetryP50: r.vanilla.zeroRetryP50,
          zeroRetryP95: r.vanilla.zeroRetryP95,
          zeroRetryMean: r.vanilla.zeroRetryMean,
          avgConnections: r.vanilla.avgConnections,
          avgBytes: r.vanilla.avgBytes,
        },
        catcher: {
          successRate: r.catcher.successRate,
          successes: r.catcher.successes,
          failures: r.catcher.failures,
          zeroRetrySuccesses: r.catcher.zeroRetrySuccesses,
          retriedSuccesses: r.catcher.retriedSuccesses,
          avgRetries: r.catcher.avgRetries,
          zeroRetryP50: r.catcher.zeroRetryP50,
          zeroRetryP95: r.catcher.zeroRetryP95,
          zeroRetryMean: r.catcher.zeroRetryMean,
          retriedMean: r.catcher.retriedMean,
          avgConnections: r.catcher.avgConnections,
          avgBytes: r.catcher.avgBytes,
        },
        improvements: r.improvements,
      })),
    }, null, 2), 'utf-8')

    const mdPath = path.join(outputDir, 'comparison-report.md')
    await fs.writeFile(mdPath, this.toMarkdown(), 'utf-8')

    console.log(`[reporter] JSON → ${jsonPath}`)
    console.log(`[reporter] Markdown → ${mdPath}`)
  }
}
