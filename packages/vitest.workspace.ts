import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: [
      'test/integration/**/*.test.ts',
      'test/e2e/**/*.test.ts',
      'test/chaos/**/*.test.ts',
    ],
    testTimeout: 180_000,
    hookTimeout: 60_000,
    reporters: ['verbose'],
  },
  benchmark: {
    include: ['test/benchmark/**/*.bench.ts'],
    reporters: ['verbose'],
  },
})
