import MutationVitestReporter from "../../sdk/mutator-vitest-reporter.ts";
import { readMutationRequest } from "../../sdk/mutator.ts";
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";

type ErrorData = { name?: string; code?: string; message?: string };
type Entity = {
  type: string; id: string; name: string;
  parent?: Entity; project: { name?: string };
  module: { moduleId: string };
  result(): { state: string; errors?: ErrorData[] };
  diagnostic(): { duration: number; retryCount: number; repeatCount: number } | undefined;
  options?: { retry?: number; repeats?: number };
};

// The product reporter remains responsible for full runner shutdown evidence.
export default class InventoryReporter extends MutationVitestReporter {
  private inventoryRequest = readMutationRequest();
  private tests: unknown[] = [];
  private inventoryProblems: string[] = [];
  private ready = new Map<string, number>();
  private results = new Map<string, number>();

  constructor() {
    super();
    process.on("uncaughtExceptionMonitor", () => {
      this.inventoryProblems.push("Uncaught exception after runner events");
      this.publishInventory();
    });
    process.once("exit", () => this.publishInventory());
  }

  onInit(context: any): void {
    super.onInit(context);
    if (context.config?.retry || context.config?.bail) {
      this.inventoryProblems.push("Configured retries or early stopping are unsupported");
    }
  }

  onTestCaseReady(test: Entity): void {
    this.ready.set(test.id, (this.ready.get(test.id) ?? 0) + 1);
  }

  onTestCaseResult(test: Entity): void {
    this.results.set(test.id, (this.results.get(test.id) ?? 0) + 1);
  }

  onTestRunEnd(modules: any, errors: any, reason: string): void {
    super.onTestRunEnd(modules, errors, reason);
    try {
      const occurrences = new Map<string, number>();
      for (const module of modules) {
        for (const test of module.children.allTests() as Iterable<Entity>) {
          if (this.tests.length >= 10000) throw new Error("Inventory exceeds event budget");
          const names: string[] = [test.name];
          let parent = test.parent;
          while (parent?.type === "suite") {
            if (names.length > 100) throw new Error("Test hierarchy exceeds budget");
            names.unshift(parent.name); parent = parent.parent;
          }
          const root = process.env.ARCHGUARD_RESEARCH_ROOT;
          if (!root) throw new Error("Research root is missing");
          const file = path.relative(root, test.module.moduleId).split(path.sep).join("/");
          if (file.startsWith("../") || path.isAbsolute(file)) throw new Error("Test outside owned source root");
          const project = test.project.name ?? "";
          const key = JSON.stringify([project, file, names]);
          const occurrence = (occurrences.get(key) ?? 0) + 1;
          occurrences.set(key, occurrence);
          const diagnostic = test.diagnostic();
          const result = test.result();
          if (typeof test.id !== "string" || typeof test.name !== "string") throw new Error("Test identity unavailable");
          const item: Record<string, unknown> = {
            id: JSON.stringify([project, file, names, occurrence]), rawId: test.id,
            project, file, name: names.join(" > "), hierarchy: names, occurrence,
            state: result.state, readyCount: this.ready.get(test.id) ?? 0,
            resultCount: this.results.get(test.id) ?? 0,
            errors: (result.errors ?? []).map(error => ({ name: error.name ?? "", code: error.code ?? "" })),
            retryCount: diagnostic?.retryCount ?? null, repeatCount: diagnostic?.repeatCount ?? null,
          };
          if (diagnostic && Number.isFinite(diagnostic.duration)) item.durationMs = diagnostic.duration;
          if (test.options?.retry || test.options?.repeats) this.inventoryProblems.push("Configured retries or repetitions are unsupported");
          this.tests.push(item);
        }
      }
    } catch (error) { this.inventoryProblems.push(String(error)); }
  }

  private publishInventory(): void {
    try {
      const destination = process.env.ARCHGUARD_RESEARCH_INVENTORY;
      if (!destination || !path.isAbsolute(destination)) throw new Error("Inventory destination unavailable");
      const request = this.inventoryRequest;
      const resultHash = createHash("sha256").update(fs.readFileSync(request.resultPath)).digest("hex");
      const encoded = JSON.stringify({
        schemaVersion: 1, requestId: request.requestId, runId: request.runId,
        inputDigest: request.inputDigest, protocolSha256: resultHash,
        tests: this.tests, problems: this.inventoryProblems,
      });
      if (Buffer.byteLength(encoded) > 8 * 1024 * 1024) throw new Error("Inventory exceeds byte budget");
      const temporary = `${destination}.tmp`;
      fs.writeFileSync(temporary, encoded, { mode: 0o600 }); fs.renameSync(temporary, destination);
    } catch { process.exitCode = 3; }
  }
}
