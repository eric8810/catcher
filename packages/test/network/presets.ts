/**
 * Pre-defined network condition profiles matching docs/simulation-before-after.md
 */

import type { NetworkConditions } from './proxy.js'

export interface NetworkProfile {
  name: string
  emoji: string
  conditions: NetworkConditions
}

export const NETWORK_PROFILES: Record<string, NetworkProfile> = {
  good: {
    name: '良好网络',
    emoji: '🟢',
    conditions: {
      latency: 25,        // 50ms RTT → 25ms one-way
      packetLoss: 0,
      bandwidth: 0,       // unlimited
      connectionReset: 0,
    },
  },
  weak: {
    name: '弱网',
    emoji: '🟡',
    conditions: {
      latency: 1000,      // 2000ms RTT → 1000ms one-way
      packetLoss: 0.05,   // 10% loss → 5% per direction approx
      bandwidth: 25_000,  // 200Kbps ≈ 25KB/s
      connectionReset: 0.02,
    },
  },
  veryWeak: {
    name: '极弱网',
    emoji: '🔴',
    conditions: {
      latency: 2000,      // 4000ms RTT → 2000ms one-way
      packetLoss: 0.1,    // 20% loss → 10% per direction
      bandwidth: 6_250,   // 50Kbps ≈ 6.25KB/s
      connectionReset: 0.05,
    },
  },
  satellite: {
    name: '卫星 WiFi',
    emoji: '✈️',
    conditions: {
      latency: 400,       // 800ms RTT → 400ms one-way
      packetLoss: 0.02,
      bandwidth: 250_000, // 2Mbps ≈ 250KB/s
      connectionReset: 0.01,
    },
  },
  mobile3g: {
    name: '偏远 3G',
    emoji: '🏔️',
    conditions: {
      latency: 1000,      // 2000ms RTT
      packetLoss: 0.08,
      bandwidth: 6_250,   // 50Kbps
      connectionReset: 0.08,
    },
  },
  crossRegion: {
    name: '跨地域 (新加坡→华东)',
    emoji: '🌍',
    conditions: {
      latency: 150,       // 300ms RTT
      packetLoss: 0.01,
      bandwidth: 0,
      connectionReset: 0,
    },
  },
  metro: {
    name: '地铁通勤 (频繁切换)',
    emoji: '🚇',
    conditions: {
      latency: 50,
      packetLoss: 0.03,
      bandwidth: 0,
      connectionReset: 0.1,  // high disruption rate
    },
  },
}

/** Get one-way latency from RTT (divides by 2) */
export function rttToLatency(rttMs: number): number {
  return Math.round(rttMs / 2)
}
