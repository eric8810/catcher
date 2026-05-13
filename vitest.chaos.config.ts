import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['packages/test/chaos/**/*.test.ts'],
    testTimeout: 120_000,
    reporters: ['verbose'],
  },
})
