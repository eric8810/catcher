import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['packages/test/benchmark/napi-*.test.ts'],
    testTimeout: 300_000,
    reporters: ['verbose'],
  },
  benchmark: {
    include: ['packages/test/benchmark/napi-*.bench.ts'],
    reporters: ['verbose'],
  },
})
