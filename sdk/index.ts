import * as fs from "node:fs";

export type ResolutionStatus = "internal" | "external" | "unresolved" | "excluded" | "outsideRoot" | "unsupported";
export interface ImportBinding { local: string; imported: string; typeOnly: boolean }
export interface ImportFact { specifier: string | null; kind: string; typeOnly: boolean; offset: number; bindings: readonly ImportBinding[]; status: ResolutionStatus; target: string | null; detail: string | null }
export interface FileFacts { path: string; bytes: number; language: string; imports: readonly ImportFact[]; exports: readonly { name: string; typeOnly: boolean; local: string | null; offset: number }[]; calls: readonly { callee: string; origin: string | null; offset: number; stringArguments: readonly (string | null)[] }[]; services: readonly { name: string; identifier: string | null; offset: number }[] }
// installed-source requires successful package lookup; source permits uninstalled external edges.
export interface ProjectFacts { schemaVersion: 1; root: string; files: readonly FileFacts[]; packages: readonly { path: string; bytes: number; name: string; dependencies: readonly string[]; exports: unknown }[]; problems: readonly { file: string; offset: number; message: string }[]; resolution: { mode: string; conditions: readonly string[]; extensions: readonly string[]; extensionAliases: readonly [string, readonly string[]][]; tsconfig: string } }
export interface Diagnostic { rule: string; file: string; offset: number; message: string; evidence?: readonly string[] }
export type ProjectRule = (project: ProjectFacts) => readonly Diagnostic[];

export function readProject(): ProjectFacts {
  const project: ProjectFacts = JSON.parse(fs.readFileSync(0, "utf8"));
  if (project.schemaVersion !== 1 || !Array.isArray(project.files) || !Array.isArray(project.problems)) throw new Error("Unsupported project facts protocol");
  return project;
}
export function runPlugin(rule: ProjectRule): void {
  process.stdout.write(JSON.stringify({ schemaVersion: 1, diagnostics: rule(readProject()) }) + "\n");
}

// Build the file lookup once when a rule queries many source files.
export function createDependencyQuery(project: ProjectFacts): (start: string, includeTypes?: boolean) => ReadonlyMap<string, readonly string[]> {
  const files = new Map(project.files.map(file => [file.path, file]));
  return (start, includeTypes = true) => {
    const paths = new Map<string, readonly string[]>();
    const queue: (readonly string[])[] = [[start]];
    const visited = new Set([start]);
    for (let index = 0; index < queue.length; index++) {
      const path = queue[index]!;
      const file = files.get(path[path.length - 1]!);
      if (!file) continue;
      for (const edge of file.imports) {
        if (edge.status !== "internal" || edge.target === null || (!includeTypes && edge.typeOnly) || visited.has(edge.target)) continue;
        visited.add(edge.target);
        const next = [...path, edge.target];
        paths.set(edge.target, next);
        queue.push(next);
      }
    }
    return paths;
  };
}

// Convenient for a single query. Reuse createDependencyQuery for many roots.
export function dependencyPaths(project: ProjectFacts, start: string, includeTypes = true): ReadonlyMap<string, readonly string[]> {
  return createDependencyQuery(project)(start, includeTypes);
}

export { readSemanticFacts, semanticFile, missingMemberDiagnostics } from "./semantic.ts";
export type { SemanticFacts, SemanticFile, PropertyFact, SemanticRule, CompilerDiagnostic } from "./semantic.ts";
export { readMutationRequest, validateMutationRequest, validateMutationResult, writeMutationResult, mutationProtocolLimits } from "./mutator.ts";
export type { TestExecutionRequest, TestExecutionResult, TestCounts, TestFailure, TestFailureKind } from "./mutator.ts";
export { default as MutationVitestReporter } from "./mutator-vitest-reporter.ts";
