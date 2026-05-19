import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['packages/test/chaos/napi-*.test.ts'],
    testTimeout: 120_000,
    reporters: ['verbose'],
  },
})
