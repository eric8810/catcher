/**
 * Pre-defined network condition profiles.
 *
 * Sources:
 *   - Chrome DevTools throttling profiles
 *   - WebPageTest connectivity.ini
 *   - Real-world mobile network measurements
 */

import type { NetworkConditions } from './proxy.js'

export interface NetworkProfile {
  name: string
  emoji: string
  conditions: NetworkConditions
}

export const NETWORK_PROFILES: Record<string, NetworkProfile> = {
  // ── v0.1 profiles (existing) ──────────────────────────────

  good: {
    name: '良好网络',
    emoji: '🟢',
    conditions: {
      latency: 25,
      jitter: 10,
      packetLoss: 0,
      bandwidth: 0,
      connectionReset: 0,
    },
  },

  weak: {
    name: '弱网',
    emoji: '🟡',
    conditions: {
      latency: 1000,
      jitter: 200,
      packetLoss: 0.05,
      bandwidth: 25_000,
      connectionReset: 0.02,
    },
  },

  veryWeak: {
    name: '极弱网',
    emoji: '🔴',
    conditions: {
      latency: 2000,
      jitter: 400,
      packetLoss: 0.1,
      bandwidth: 6_250,
      connectionReset: 0.05,
    },
  },

  satellite: {
    name: '卫星 WiFi',
    emoji: '✈️',
    conditions: {
      latency: 400,
      jitter: 80,
      packetLoss: 0.02,
      bandwidth: 250_000,
      connectionReset: 0.01,
    },
  },

  mobile3g: {
    name: '偏远 3G',
    emoji: '🏔️',
    conditions: {
      latency: 1000,
      jitter: 200,
      packetLoss: 0.08,
      bandwidth: 6_250,
      connectionReset: 0.08,
    },
  },

  crossRegion: {
    name: '跨地域 (新加坡→华东)',
    emoji: '🌍',
    conditions: {
      latency: 150,
      jitter: 30,
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
      jitter: 25,
      packetLoss: 0.03,
      bandwidth: 0,
      connectionReset: 0.1,
    },
  },

  // ── v0.2: Chrome DevTools / WebPageTest standard profiles ──

  gprs: {
    name: 'GPRS (2.5G)',
    emoji: '📟',
    conditions: {
      latency: 250,
      jitter: 100,
      packetLoss: 0.02,
      bandwidth: 6_250,
      connectionReset: 0.03,
      upload: {
        bandwidth: 2_500,
        packetLoss: 0.03,
      },
    },
  },

  '2g_regular': {
    name: '2G Regular',
    emoji: '📶',
    conditions: {
      latency: 150,
      jitter: 50,
      packetLoss: 0.01,
      bandwidth: 31_250,
      connectionReset: 0.02,
      upload: {
        bandwidth: 6_250,
        packetLoss: 0.02,
      },
    },
  },

  '2g_good': {
    name: '2G Good',
    emoji: '📶',
    conditions: {
      latency: 75,
      jitter: 30,
      packetLoss: 0.005,
      bandwidth: 56_250,
      connectionReset: 0.01,
      upload: {
        bandwidth: 18_750,
        packetLoss: 0.01,
      },
    },
  },

  '3g_slow': {
    name: '3G Slow',
    emoji: '📱',
    conditions: {
      latency: 100,
      jitter: 40,
      packetLoss: 0.005,
      bandwidth: 97_500,
      connectionReset: 0.01,
      upload: {
        bandwidth: 41_250,
        packetLoss: 0.01,
      },
    },
  },

  '3g_good': {
    name: '3G Good',
    emoji: '📱',
    conditions: {
      latency: 20,
      jitter: 10,
      packetLoss: 0,
      bandwidth: 187_500,
      connectionReset: 0,
      upload: {
        bandwidth: 93_750,
      },
    },
  },

  '4g_lte': {
    name: '4G/LTE',
    emoji: '📡',
    conditions: {
      latency: 10,
      jitter: 5,
      packetLoss: 0,
      bandwidth: 500_000,
      connectionReset: 0,
      upload: {
        bandwidth: 375_000,
      },
    },
  },

  dsl: {
    name: 'DSL 宽带',
    emoji: '🏠',
    conditions: {
      latency: 3,
      jitter: 2,
      packetLoss: 0,
      bandwidth: 250_000,
      connectionReset: 0,
      upload: {
        bandwidth: 125_000,
      },
    },
  },

  // ── Chaos profiles ─────────────────────────────────────────

  burst_storm: {
    name: '突发丢包风暴',
    emoji: '🌪️',
    conditions: {
      latency: 25,
      packetLoss: 0,
      burstLoss: {
        p_good_to_bad: 0.03,
        p_bad_to_good: 0.15,
        loss_good: 0.01,
        loss_bad: 0.6,
      },
    },
  },

  blackhole_30s: {
    name: '路由黑洞 30s',
    emoji: '🕳️',
    conditions: {
      latency: 25,
      packetLoss: 0,
      blackhole: {
        enabled: true,
        duration: 30_000,
        destroyOnRecover: true,
      },
    },
  },

  blackhole_intermittent: {
    name: '间歇黑洞 (10s×5)',
    emoji: '🕳️',
    conditions: {
      latency: 25,
      packetLoss: 0,
      // Handled programmatically in scenarios
    },
  },

  asymmetric_2g: {
    name: '2G 严重不对称',
    emoji: '⚖️',
    conditions: {
      download: {
        latency: 75,
        bandwidth: 56_250,
        packetLoss: 0.01,
      },
      upload: {
        latency: 250,
        bandwidth: 6_250,
        packetLoss: 0.05,
      },
    },
  },
}

/** Get one-way latency from RTT (divides by 2) */
export function rttToLatency(rttMs: number): number {
  return Math.round(rttMs / 2)
}
