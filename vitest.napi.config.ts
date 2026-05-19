import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: [
      'packages/test/integration/napi.test.ts',
      'packages/test/integration/napi-dns.test.ts',
    ],
    testTimeout: 30_000,
    hookTimeout: 15_000,
    reporters: ['verbose'],
  },
})
