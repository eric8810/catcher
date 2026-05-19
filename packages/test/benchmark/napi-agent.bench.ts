/**
 * Micro-benchmark: NAPI HttpClient construction cost.
 *
 * Mirrors agent.bench.ts — measures construction overhead:
 *   - Vanilla: new https.Agent (Node.js built-in)
 *   - NAPI: new HttpClient (Rust client via napi-rs)
 *
 * The NAPI HttpClient builds a full reqwest::Client + StaleAwareDnsResolver
 * on construction, so this measures Rust initialization overhead.
 */

import https from 'node:https'
import { bench, describe } from 'vitest'
import { HttpClient } from '@eric8810/catcher-napi-http'

// ── Client creation overhead ────────────────────────────────

describe('napi — client creation cost', () => {
  bench('new https.Agent (default)', () => {
    new https.Agent({ keepAlive: false })
  })

  bench('new HttpClient (NAPI, minimal config)', () => {
    new HttpClient(JSON.stringify({
      base_url: '',
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
    }))
  })

  bench('new HttpClient (NAPI, full config)', () => {
    new HttpClient(JSON.stringify({
      base_url: 'http://localhost',
      connect_timeout_ms: 5000,
      response_timeout_ms: 30000,
      pool: {
        keep_alive: true,
        keep_alive_interval_secs: 60,
        max_idle_per_host: 10,
        idle_timeout_secs: 90,
      },
      dns: {
        cache_size: 512,
        cache_ttl_secs: 300,
        negative_ttl_secs: 60,
        stale_ttl_secs: 3600,
        stale_on_error: true,
      },
      retry: {
        max_attempts: 3,
        backoff: 'Exponential',
        min_backoff_ms: 100,
        max_backoff_ms: 10000,
        jitter: true,
      },
      max_concurrency: 50,
    }))
  })
})

// ── NOTE: Codec benchmark ─────────────────────────────────────
//
// catcher-napi-ws does not expose standalone pack/unpack functions.
// Rust rmp-serde codec is only used internally for WS binary messages.
// To benchmark Rust vs JS msgpack, pack/unpack need to be exported
// from @eric8810/catcher-napi-ws first.
//
// See codec.bench.ts for the TS (msgpackr) vs JSON baseline.
