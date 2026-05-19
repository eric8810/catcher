import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['packages/test/chaos/napi-*.test.ts'],
    testTimeout: 600_000,
    reporters: ['verbose'],
  },
})
