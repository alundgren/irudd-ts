import { defineConfig } from "vite-plus/test/config";
export default defineConfig({ test: {
  include: ["packages/shared/src/{path,hostClassification,delimitedPreview,gitPatchPath}.test.ts", "quality-controls/*.test.ts"],
  pool: "forks", maxWorkers: 1, fileParallelism: false,
  reporters: ["./sdk/mutator-vitest-reporter.ts"],
}});
