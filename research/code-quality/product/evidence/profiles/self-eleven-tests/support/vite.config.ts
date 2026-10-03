import { defineConfig } from "vite-plus/test/config";
export default defineConfig({ test: { include: ["quality-tests/*.test.ts"],
  pool: "forks", maxWorkers: 1, fileParallelism: false,
  reporters: ["./quality-support/mutator-vitest-reporter.ts"],
}});
