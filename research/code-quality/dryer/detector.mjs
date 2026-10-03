import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

// An independent syntax experiment. It does not use compiler types.
const functions = new Set([
  ts.SyntaxKind.FunctionDeclaration, ts.SyntaxKind.MethodDeclaration,
  ts.SyntaxKind.ArrowFunction, ts.SyntaxKind.FunctionExpression,
]);

function normalize(node, preserveMembers, parent) {
  const kind = ts.SyntaxKind[node.kind];
  if (ts.isIdentifier(node)) {
    const called = parent && ((ts.isCallExpression(parent) || ts.isNewExpression(parent)) && parent.expression === node);
    const member = parent && ts.isPropertyAccessExpression(parent) && parent.name === node;
    const memberCall = member && parent.parent &&
      (ts.isCallExpression(parent.parent) || ts.isNewExpression(parent.parent)) && parent.parent.expression === parent;
    const property = parent && (ts.isPropertyAssignment(parent) || ts.isShorthandPropertyAssignment(parent)) && parent.name === node;
    return [kind, called || memberCall || (preserveMembers && (member || property)) ? node.text : '$local'];
  }
  if (ts.isLiteralExpression(node) || [ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword, ts.SyntaxKind.NullKeyword].includes(node.kind)) {
    return [kind, '$literal'];
  }
  const children = [];
  // The compiler stores unary operators outside its child traversal.
  if (ts.isPrefixUnaryExpression(node) || ts.isPostfixUnaryExpression(node)) {
    children.push(['operator', ts.SyntaxKind[node.operator]]);
  }
  ts.forEachChild(node, child => { children.push(normalize(child, preserveMembers, node)); });
  return [kind, ...children];
}

export function inventory(tree) {
  const counts = new Map();
  const sizes = new Map();
  function visit(item) {
    const size = 1 + item.filter(Array.isArray).reduce((n, child) => n + visit(child), 0);
    const key = JSON.stringify(item);
    counts.set(key, (counts.get(key) ?? 0) + 1);
    sizes.set(key, size);
    return size;
  }
  visit(tree);
  return { counts, sizes };
}

export function similarities(left, right) {
  const keys = new Set([...left.counts.keys(), ...right.counts.keys()]);
  let shared = 0, union = 0, multiShared = 0, multiUnion = 0, weightedShared = 0, weightedUnion = 0;
  for (const key of keys) {
    const a = left.counts.get(key) ?? 0, b = right.counts.get(key) ?? 0;
    const weight = left.sizes.get(key) ?? right.sizes.get(key);
    shared += a > 0 && b > 0 ? 1 : 0;
    union++;
    multiShared += Math.min(a, b);
    multiUnion += Math.max(a, b);
    weightedShared += Math.min(a, b) * weight;
    weightedUnion += Math.max(a, b) * weight;
  }
  return { set: shared / (union || 1), multiset: multiShared / (multiUnion || 1), weighted: weightedShared / (weightedUnion || 1) };
}

export function extract(source, file) {
  const parsed = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const diagnostics = parsed.parseDiagnostics.map(d => ({
    file, code: d.code, message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
    line: parsed.getLineAndCharacterOfPosition(d.start ?? 0).line + 1,
  }));
  if (diagnostics.length) return { entries: [], diagnostics };
  const entries = [];
  function visit(node) {
    if (functions.has(node.kind)) {
      if (!node.body) return;
      const standalone = ts.isFunctionDeclaration(node) || ts.isMethodDeclaration(node) ||
        (node.parent && (ts.isVariableDeclaration(node.parent) || ts.isPropertyDeclaration(node.parent)));
      if (standalone) {
        const selected = ts.isArrowFunction(node) || ts.isFunctionExpression(node) ? node.parent : node;
        const start = parsed.getLineAndCharacterOfPosition(selected.getStart(parsed)).line + 1;
        const end = parsed.getLineAndCharacterOfPosition(selected.end).line + 1;
        const erased = inventory(normalize(selected, false));
        const preserved = inventory(normalize(selected, true));
        const nodes = [...erased.counts.values()].reduce((a, b) => a + b, 0);
        entries.push({ file, name: selected.name?.getText(parsed) ?? '<anonymous>', start, end, nodes, erased, preserved });
      }
      return; // Nested callbacks belong to the enclosing function only.
    }
    ts.forEachChild(node, visit);
  }
  visit(parsed);
  return { entries, diagnostics };
}

export function scan(files, root, { minLines = 4, minNodes = 20, thresholds = [0.7, 0.75, 0.8, 0.82, 0.85, 0.9, 0.95] } = {}) {
  const entries = [], excluded = [], diagnostics = [];
  for (const file of files) {
    const relative = path.relative(root, file).split(path.sep).join('/');
    try {
      const found = extract(fs.readFileSync(file, 'utf8'), relative);
      diagnostics.push(...found.diagnostics);
      for (const entry of found.entries) {
        if (entry.end - entry.start + 1 >= minLines && entry.nodes >= minNodes) entries.push(entry);
        else excluded.push({ file: entry.file, name: entry.name, start: entry.start, end: entry.end, nodes: entry.nodes, reason: 'minimum size' });
      }
    } catch (error) { diagnostics.push({ file: relative, message: error.message, code: 'read-failure' }); }
  }
  const location = e => ({ file: e.file, name: e.name, start: e.start, end: e.end, nodes: e.nodes });
  const pairs = [];
  for (let i = 0; i < entries.length; i++) for (let j = i + 1; j < entries.length; j++) {
    const a = entries[i], b = entries[j];
    pairs.push({ left: location(a), right: location(b), erased: similarities(a.erased, b.erased), preserved: similarities(a.preserved, b.preserved) });
  }
  pairs.sort((a, b) => b.preserved.set - a.preserved.set);
  const sensitivity = thresholds.map(threshold => ({ threshold,
    erasedSet: pairs.filter(p => p.erased.set >= threshold).length,
    preservedSet: pairs.filter(p => p.preserved.set >= threshold).length,
    preservedMultiset: pairs.filter(p => p.preserved.multiset >= threshold).length,
    preservedWeighted: pairs.filter(p => p.preserved.weighted >= threshold).length,
  }));
  return { status: diagnostics.length ? 'incomplete' : 'complete', tool: `independent-ts-${ts.version}`, config: { minLines, minNodes, thresholds }, diagnostics, entries: entries.map(location), excluded, pairs, sensitivity };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [root, ...files] = process.argv.slice(2);
  if (!root || !files.length) { console.error('Usage: node detector.mjs ROOT FILE...'); process.exitCode = 2; }
  else {
    const result = scan(files.map(file => path.resolve(root, file)), path.resolve(root));
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.status === 'complete' ? 0 : 2;
  }
}
