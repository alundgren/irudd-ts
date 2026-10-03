import { defineConfig } from 'vitest/config';
export default defineConfig({
  test: {
    include: ['src/*.test.ts'],
    maxWorkers: 1,
    coverage: { provider: 'v8', reporter: ['lcov'], include: ['src/account.ts', 'src/customSnoozeDate.ts'] }
  }
});
