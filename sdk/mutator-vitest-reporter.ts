import {
  readMutationRequest, writeMutationResult,
} from "./mutator.ts";
import type { TestExecutionRequest, TestExecutionResult, TestFailure, TestFailureKind } from "./mutator.ts";

type RunnerError = { name?: unknown; code?: unknown; message?: unknown };
type TestCase = {
  id: string;
  type: "test";
  module: { moduleId: string };
  result(): { state: string; errors?: readonly RunnerError[] };
};
type Suite = {
  type: string;
  moduleId?: string;
  module?: { moduleId: string };
  errors(): readonly RunnerError[];
  children: { allTests(): Iterable<TestCase>; allSuites(): Iterable<Suite> };
};
type HookEntity = {
  task?: { result?: { hooks?: Record<string, string>; errors?: readonly RunnerError[] } };
  moduleId?: string;
  module?: { moduleId: string };
};
type RunnerContext = {
  config: { watch: boolean; mergeReports?: unknown };
  close(): Promise<void>;
  waitForTestRunEnd(): Promise<unknown>;
  state: { getUnhandledErrors(): readonly RunnerError[]; blobs?: unknown };
  logger: { error(...arguments_: unknown[]): unknown };
};

const MAX_EVENTS = 10_000;
let reporterCreated = false;

function boundedMessage(value: unknown): string {
  const string = typeof value === "string" ? value : "Test runner error";
  let bounded = string.slice(0, 4_000);
  while (Buffer.byteLength(bounded) > 4_000) bounded = bounded.slice(0, -1);
  return bounded.length < string.length ? `${bounded} [truncated]` : bounded;
}
function message(error: RunnerError): string { return boundedMessage(error?.message); }
function assertion(error: RunnerError): boolean {
  return error?.name === "AssertionError" || error?.name === "AssertionError [ERR_ASSERTION]" || error?.code === "ERR_ASSERTION";
}
function file(entity: { moduleId?: string; module?: { moduleId: string } }): string | null {
  return entity.moduleId ?? entity.module?.moduleId ?? null;
}

export default class MutationVitestReporter {
  private request: TestExecutionRequest;
  private context: RunnerContext | undefined;
  private modules: readonly Suite[] = [];
  private hooks: { name: string; entity: HookEntity }[] = [];
  private observedHooks = new WeakMap<HookEntity, Set<string>>();
  private earlyErrors: RunnerError[] = [];
  private infrastructure: TestFailure[] = [];
  private runEnded = false;
  private runSettled = false;
  private closeStarted = false;
  private closeFinished = false;
  private closing = false;
  private supported = true;
  private interrupted = false;
  private closeBridge: (() => Promise<void>) | undefined;
  private loggerBridge: ((...arguments_: unknown[]) => unknown) | undefined;
  private waitMethod: (() => Promise<unknown>) | undefined;

  constructor() {
    this.request = readMutationRequest();
    if (reporterCreated) throw new Error("Mutation reporter supports one one-shot runner per process");
    reporterCreated = true;
    process.on("uncaughtExceptionMonitor", error => this.addInfrastructure("unhandled", message(error)));
    process.once("exit", exitCode => this.finalize(exitCode));
  }

  private addInfrastructure(kind: TestFailureKind, detail: string): void {
    if (this.infrastructure.length >= MAX_EVENTS) { this.supported = false; return; }
    this.infrastructure.push({ kind, testId: null, file: null, message: boundedMessage(detail) });
  }

  onInit(context: RunnerContext): void {
    if (!context || typeof context.close !== "function" || typeof context.waitForTestRunEnd !== "function" || typeof context.state?.getUnhandledErrors !== "function" || typeof context.logger?.error !== "function") {
      this.supported = false;
      this.addInfrastructure("runtime", "Unsupported test runner lifecycle API");
      return;
    }
    this.context = context;
    if (context.config?.watch !== false || context.config.mergeReports || context.state.blobs !== undefined) {
      this.supported = false;
      this.addInfrastructure("runtime", "Mutation reporter requires a fresh one-shot test run");
    }
    const originalClose = context.close;
    const originalError = context.logger.error;
    this.waitMethod = context.waitForTestRunEnd;
    this.closeBridge = async () => {
      this.closeStarted = true;
      this.closing = true;
      try { await originalClose.call(context); this.closeFinished = true; }
      catch (error) { this.addInfrastructure("runtime", message(error as RunnerError)); throw error; }
      finally { this.closing = false; }
    };
    this.loggerBridge = (...arguments_: unknown[]) => {
      // Vitest logs swallowed cleanup errors instead of retaining them in run state.
      if (this.closing) this.addInfrastructure("runtime", "Test runner cleanup reported an error");
      return originalError.apply(context.logger, arguments_);
    };
    context.close = this.closeBridge;
    context.logger.error = this.loggerBridge;
  }

  onHookStart(hook: { name: string; entity: HookEntity }): void { this.observeHook(hook); }
  onHookEnd(hook: { name: string; entity: HookEntity }): void { this.observeHook(hook); }

  private observeHook(hook: { name: string; entity: HookEntity }): void {
    const names = this.observedHooks.get(hook.entity) ?? new Set<string>();
    if (names.has(hook.name)) return;
    if (this.hooks.length >= MAX_EVENTS) { this.supported = false; return; }
    names.add(hook.name);
    this.observedHooks.set(hook.entity, names);
    this.hooks.push(hook);
  }

  onTestRunEnd(modules: readonly Suite[], errors: readonly RunnerError[], reason: string): void {
    if (this.runEnded || modules.length > MAX_EVENTS || errors.length > MAX_EVENTS) { this.supported = false; return; }
    this.runEnded = true;
    this.interrupted = reason === "interrupted";
    if (reason !== "passed" && reason !== "failed" && reason !== "interrupted") this.supported = false;
    this.modules = modules;
    this.earlyErrors = [...errors];
    const context = this.context;
    if (!context || context.waitForTestRunEnd !== this.waitMethod) { this.supported = false; return; }
    // Awaiting this promise inside the callback would wait on this callback itself.
    try {
      const pending = context.waitForTestRunEnd();
      if (!pending || typeof pending.then !== "function") { this.supported = false; return; }
      pending.then(() => { this.runSettled = true; }, error => {
        this.addInfrastructure("runtime", message(error as RunnerError));
      });
    } catch (error) { this.addInfrastructure("runtime", message(error as RunnerError)); }
  }

  onProcessTimeout(): void {
    this.supported = false;
    this.addInfrastructure("runtime", "Test runner cleanup timed out");
  }

  private collect(exitCode: number): TestExecutionResult {
    const failures: TestFailure[] = [...this.infrastructure];
    const tests = { passed: 0, failed: 0, skipped: 0 };
    const seen = new Set<string>();
    let visited = 0;
    const reserve = (count: number): boolean => {
      if (count > MAX_EVENTS * 3 - visited) { this.supported = false; return false; }
      visited += count;
      return true;
    };
    const add = (kind: TestFailureKind, error: RunnerError, testId: string | null, fileName: string | null) => {
      if (failures.length >= MAX_EVENTS) { this.supported = false; return; }
      failures.push({ kind, testId, file: fileName, message: message(error) });
    };
    const addErrors = (kind: TestFailureKind, errors: readonly RunnerError[], testId: string | null, fileName: string | null) => {
      if (!Array.isArray(errors) || errors.length > MAX_EVENTS || !reserve(errors.length)) { this.supported = false; return; }
      for (const error of errors) add(kind, error, testId, fileName);
    };
    for (const module of this.modules) {
      if (!reserve(1)) break;
      if (!module.children || typeof module.children.allTests !== "function" || typeof module.children.allSuites !== "function" || typeof module.errors !== "function") { this.supported = false; continue; }
      addErrors("import", module.errors(), null, file(module));
      for (const suite of module.children.allSuites()) {
        if (!reserve(1) || typeof suite.errors !== "function") { this.supported = false; break; }
        addErrors("suite", suite.errors(), null, file(suite));
      }
      for (const test of module.children.allTests()) {
        if (!reserve(1) || seen.size >= MAX_EVENTS || typeof test.id !== "string" || seen.has(test.id) || typeof test.result !== "function") { this.supported = false; break; }
        seen.add(test.id);
        const result = test.result();
        if (result.state === "passed") {
          tests.passed++;
          if (result.errors?.length) { this.supported = false; add("runtime", { message: "A retried passing test retained failures" }, null, file(test)); }
        } else if (result.state === "skipped") tests.skipped++;
        else if (result.state === "failed") {
          tests.failed++;
          if (!result.errors?.length) { this.supported = false; add("runtime", { message: "Failed test lacks error metadata" }, test.id, file(test)); }
          if (!Array.isArray(result.errors) || result.errors.length > MAX_EVENTS || !reserve(result.errors.length)) this.supported = false;
          else for (const error of result.errors) add(assertion(error) ? "assertion" : "runtime", error, test.id, file(test));
        } else this.supported = false;
      }
    }
    for (const hook of this.hooks) {
      // The supported runner exposes this data through experimental_getRunnerTask.
      const result = hook.entity.task?.result;
      const state = result?.hooks?.[hook.name];
      // A throwing Vitest hook keeps "run" and never emits onHookEnd.
      if (state === "fail" || (state === "run" && result?.errors?.length)) add("hook", { message: `Test hook ${hook.name} failed` }, null, file(hook.entity));
      else if (state !== "pass") { this.supported = false; add("hook", { message: "Hook completion metadata unavailable" }, null, file(hook.entity)); }
    }
    const context = this.context;
    if (!context || context.close !== this.closeBridge || context.logger.error !== this.loggerBridge || context.waitForTestRunEnd !== this.waitMethod) this.supported = false;
    if (context) {
      const errors = context.state.getUnhandledErrors();
      if (!Array.isArray(errors) || errors.length > MAX_EVENTS) this.supported = false;
      else for (const error of errors) add("unhandled", error, null, null);
    }
    for (const error of this.earlyErrors) add("unhandled", error, null, null);
    const complete = this.supported && this.runEnded && this.runSettled && this.closeStarted && this.closeFinished && !this.interrupted;
    return {
      schemaVersion: 1, requestId: this.request.requestId, runId: this.request.runId, inputDigest: this.request.inputDigest,
      complete, exitCode, reason: this.interrupted ? "interrupted" : complete ? "finished" : "infrastructureError", tests, failures,
    };
  }

  private finalize(exitCode: number): void {
    try { writeMutationResult(this.request, this.collect(exitCode)); }
    catch {
      try {
        writeMutationResult(this.request, {
          schemaVersion: 1, requestId: this.request.requestId, runId: this.request.runId, inputDigest: this.request.inputDigest,
          complete: false, exitCode, reason: "infrastructureError", tests: { passed: 0, failed: 0, skipped: 0 },
          failures: [{ kind: "runtime", testId: null, file: null, message: "Reporter metadata unavailable or exceeded its budget" }],
        });
      } catch { process.stderr.write("Archguard mutation reporter could not write its result\n"); }
    }
  }
}
