import * as fs from "node:fs";
import type { Diagnostic } from "./index.ts";
export interface PropertySite { offset: number; member: string }
export interface PropertyFact extends PropertySite {
  receiver: {state: "known" | "any" | "unknown" | "error" | "unavailable"; display: string | null};
  status: "present" | "missing" | "unavailable";
  symbol: { name: string; declarations: readonly {file: string; offset: number}[] } | null;
  detail: string | null;
}
export interface SemanticFile {path: string; bytes: number; sha256: string; available: boolean; properties: readonly PropertyFact[]}
export interface CompilerDiagnostic {phase: string; category: "error" | "warning" | "suggestion" | "message"; code: number; file: string | null; offset: number; end: number; message: string}
export interface SemanticFacts {
  schemaVersion: 1; root: string; complete: boolean;
  backend: {name: "typescript"; version: "7.0.2"; api: "typescript/unstable/sync"};
  contexts: readonly {id: string; tsconfig: string; configSha256: string; complete: boolean; files: readonly SemanticFile[]; diagnostics: readonly CompilerDiagnostic[]; problems: readonly string[]}[];
}
export type SemanticRule = (facts: SemanticFacts) => readonly Diagnostic[];
// Consume host-validated semantic-facts output. This reader is not a provider trust boundary.
export function readSemanticFacts(): SemanticFacts {
  const facts: SemanticFacts = JSON.parse(fs.readFileSync(0, "utf8"));
  if(facts.schemaVersion !== 1 || facts.backend?.version !== "7.0.2" || facts.backend.api !== "typescript/unstable/sync" || !Array.isArray(facts.contexts)) throw new Error("Unsupported semantic facts protocol");
  return facts;
}
export function semanticFile(facts: SemanticFacts, context: string, file: string): SemanticFile | undefined {
  return facts.contexts.find(item => item.id === context)?.files.find(item => item.path === file);
}
export function missingMemberDiagnostics(facts: SemanticFacts, rule = "missing-member"): readonly Diagnostic[] {
  const diagnostics: Diagnostic[] = [];
  for (const context of facts.contexts) for (const file of context.files) for (const property of file.properties) {
    if(property.receiver.state === "known" && property.status === "missing") diagnostics.push({rule,file:file.path,offset:property.offset,message:`member ${property.member} is absent from the inferred receiver type`,evidence:[`compiler context ${context.id}`,property.receiver.display ?? ""]});
  }
  return diagnostics.sort((a,b) => Buffer.compare(Buffer.from(a.rule),Buffer.from(b.rule)) || Buffer.compare(Buffer.from(a.file),Buffer.from(b.file)) || a.offset-b.offset || Buffer.compare(Buffer.from(a.message),Buffer.from(b.message)) || Buffer.compare(Buffer.from(JSON.stringify(a.evidence)),Buffer.from(JSON.stringify(b.evidence))));
}
