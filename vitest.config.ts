import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // Default: integration tests + SSE unit/integration tests (fast, stable, no Rust required)
    include: [
      'packages/test/integration/**/*.test.ts',
      'packages/catcher-http-ts/src/**/__tests__/**/*.test.ts',
      'packages/catcher-ws-ts/src/**/__tests__/**/*.test.ts',
    ],
    exclude: ['packages/test/integration/napi.test.ts'],
    testTimeout: 30_000,
    hookTimeout: 15_000,
    reporters: ['verbose'],
  },
  benchmark: {
    include: ['packages/test/benchmark/**/*.bench.ts'],
    reporters: ['verbose'],
  },
})
