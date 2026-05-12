export default {
  test: {
    include: [
      'packages/test/integration/**/*.test.ts',
      'packages/test/e2e/**/*.test.ts',
      'packages/test/chaos/**/*.test.ts',
      'packages/test/benchmark/**/*.test.ts',
    ],
    testTimeout: 180_000,
    hookTimeout: 60_000,
    reporters: ['verbose'],
  },
  benchmark: {
    include: ['packages/test/benchmark/**/*.bench.ts'],
    reporters: ['verbose'],
  },
}
