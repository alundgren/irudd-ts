import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { version } from 'typescript';
import { API, TypeFlags, DiagnosticCategory } from 'typescript/unstable/sync';
import { isPropertyAccessExpression, isIdentifier } from 'typescript/unstable/ast/is';
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const offset = (text, position) => Buffer.byteLength(text.slice(0, Math.max(0, position)), 'utf8');
const request = JSON.parse(fs.readFileSync(0, 'utf8'));
if (request.schemaVersion !== 1 || version !== '7.0.2' || request.backend.version !== version || request.backend.api !== 'typescript/unstable/sync') throw new Error('Unsupported semantic provider protocol/backend');
const api = new API({ cwd: request.root });
const contexts = [];
const unavailable = (site, detail) => ({ ...site, receiver: { state: 'unavailable', display: null }, status: 'unavailable', symbol: null, detail });
function diagnostic(phase, d, program) {
  const text = d.fileName ? program.getSourceFile(d.fileName)?.text ?? fs.readFileSync(d.fileName, 'utf8') : '';
  // Chained diagnostics remain in the message, and related locations are retained as separate entries.
  const message = [d.text, ...(d.messageChain ?? []).map(x => x.text)].join('\n');
  return { phase, category: DiagnosticCategory[d.category].toLowerCase(), code: d.code, file: d.fileName ?? null, offset: offset(text, d.pos), end: offset(text, d.end), message };
}
try {
  for (const context of request.contexts) {
    const result = { id: context.id, tsconfig: context.tsconfig, configSha256: context.configSha256, complete: false, files: [], diagnostics: [], problems: [] };
    let snapshot;
    try {
      if (hash(fs.readFileSync(context.tsconfig)) !== context.configSha256) throw new Error('Compiler configuration changed');
      snapshot = api.updateSnapshot({ openProjects: [context.tsconfig], fileChanges: { invalidateAll: true } });
      const project = snapshot.getProject(context.tsconfig);
      if (!project) throw new Error('Explicit compiler project unavailable');
      if (project.compilerOptions.runExternalCode || project.compilerOptions.contentMapper) throw new Error('External compiler code is outside this provider contract');
      const phases = { config: 'getConfigFileParsingDiagnostics', program: 'getProgramDiagnostics', global: 'getGlobalDiagnostics', syntax: 'getSyntacticDiagnostics', bind: 'getBindDiagnostics', semantic: 'getSemanticDiagnostics' };
      for (const [phase, method] of Object.entries(phases)) {
        for (const d of project.program[method]()) {
          result.diagnostics.push(diagnostic(phase, d, project.program));
          for (const related of d.relatedInformation ?? []) result.diagnostics.push(diagnostic(phase, related, project.program));
        }
      }
      for (const file of context.files) {
        const row = { path: file.path, bytes: file.bytes, sha256: file.sha256, available: false, properties: [] };
        try {
          const fileName = path.join(request.root, file.path);
          const source = project.program.getSourceFile(fileName);
          if (!source) throw new Error('Selected source is outside the explicit compiler project');
          if (hash(Buffer.from(source.text, 'utf8')) !== file.sha256 || hash(fs.readFileSync(fileName)) !== file.sha256) throw new Error('Selected source changed');
          row.available = true;
          const nodes = new Map();
          const visit = node => {
            if (isPropertyAccessExpression(node) && isIdentifier(node.name)) nodes.set(offset(source.text, node.name.getStart()), node);
            node.forEachChild(visit);
          };
          visit(source);
          // Batch receiver lookups; one context remains alive for the entire file.
          const requested = file.sites.map(site => nodes.get(site.offset));
          const valid = requested.filter(Boolean);
          const types = valid.length ? project.checker.getTypeAtLocation(valid.map(node => node.expression)) : [];
          const receivers = new Map(valid.map((node, index) => [node, types[index]]));
          for (const site of file.sites) {
            try {
              const node = nodes.get(site.offset);
              if (!node || node.name.text !== site.member) throw new Error('Requested syntax site unavailable');
              const type = receivers.get(node);
              if (!type) throw new Error('Receiver type unavailable');
              const state = type.isErrorType() ? 'error' : type.flags & TypeFlags.Any ? 'any' : type.flags & TypeFlags.Unknown ? 'unknown' : 'known';
              const display = project.checker.typeToString(type);
              if (state !== 'known') {
                row.properties.push({ ...site, receiver: { state, display }, status: 'unavailable', symbol: null, detail: `Receiver is ${state}` });
                continue;
              }
              // The actual access handles optional chains and union member resolution.
              const member = project.checker.getSymbolAtLocation(node.name) ?? project.checker.getPropertyOfType(type, site.member);
              if (!member && project.checker.getIndexInfosOfType(project.checker.getNonNullableType(type) ?? type).length) {
                row.properties.push(unavailable(site, 'Index signature access has no resolved named member symbol'));
                continue;
              }
              const declarations = (member?.declarations ?? []).map(handle => {
                const declaration = handle.resolve(project);
                if (!declaration) throw new Error('Member declaration unavailable');
                const target = project.program.getSourceFile(handle.path);
                if (!target) throw new Error('Member declaration source unavailable');
                return { file: handle.path, offset: offset(target.text, declaration.getStart()) };
              });
              row.properties.push({ ...site, receiver: { state, display }, status: member ? 'present' : 'missing', symbol: member ? { name: member.name, declarations } : null, detail: null });
            } catch (error) { row.properties.push(unavailable(site, String(error))); }
          }
        } catch (error) { row.properties = file.sites.map(site => unavailable(site, String(error))); result.problems.push(`${file.path}: ${error}`); }
        result.files.push(row);
      }
    } catch (error) {
      result.problems.push(String(error));
      result.files = context.files.map(file => ({ path: file.path, bytes: file.bytes, sha256: file.sha256, available: false, properties: file.sites.map(site => unavailable(site, String(error))) }));
    } finally { snapshot?.dispose(); }
    result.complete = result.problems.length === 0 && !result.diagnostics.some(d => d.category === 'error') && result.files.every(file => file.available && file.properties.every(fact => fact.status !== 'unavailable'));
    contexts.push(result);
  }
} finally { api.close(); }
process.stdout.write(JSON.stringify({ schemaVersion: 1, root: request.root, backend: { name: 'typescript', version, api: 'typescript/unstable/sync' }, complete: contexts.every(context => context.complete), contexts }) + '\n');
