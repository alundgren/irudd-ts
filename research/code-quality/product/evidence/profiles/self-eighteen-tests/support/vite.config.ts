import { defineConfig } from "vite-plus/test/config";
export default defineConfig({ test: { include: ["quality-tests/*.test.ts"],
  pool: "forks", maxWorkers: 1, fileParallelism: false, testTimeout: 15000,
  reporters: ["./quality-support/mutator-vitest-reporter.ts"],
}});
