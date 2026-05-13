import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // Default: only integration tests (fast, stable)
    include: ['packages/test/integration/**/*.test.ts'],
    testTimeout: 30_000,
    hookTimeout: 15_000,
    reporters: ['verbose'],
  },
  benchmark: {
    include: ['packages/test/benchmark/**/*.bench.ts'],
    reporters: ['verbose'],
  },
})
