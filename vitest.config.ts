import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // Default: only integration tests (fast, stable, no Rust required)
    include: ['packages/test/integration/**/*.test.ts'],
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
