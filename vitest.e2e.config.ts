import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['packages/test/e2e/**/*.test.ts'],
    testTimeout: 60_000,
    reporters: ['verbose'],
  },
})
