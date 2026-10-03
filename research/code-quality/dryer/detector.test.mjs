import test from 'node:test';
import assert from 'node:assert/strict';
import { extract, similarities, inventory, scan } from './detector.mjs';

const entry = source => extract(source, 'control.ts').entries[0];
test('renamed locals and literal changes still compare exactly', () => {
  const a = entry('function one(x:number) { const y = x + 1; return Math.abs(y); }');
  const b = entry('function two(a:number) { const b = a + 9; return Math.abs(b); }');
  assert.equal(similarities(a.preserved, b.preserved).set, 1);
});
test('property erasure loses a meaningful difference and preserving members corrects it', () => {
  const a = entry('function debit(x:Account) { return x.debit + x.fee; }');
  const b = entry('function credit(x:Account) { return x.credit + x.bonus; }');
  assert.equal(similarities(a.erased, b.erased).set, 1);
  assert.ok(similarities(a.preserved, b.preserved).set < 1);
  const c = entry('function debitCopy(y:Account) { return y.debit + y.fee; }');
  assert.equal(similarities(a.preserved, c.preserved).set, 1);
  const d = entry('function first() { const debit = 1; return { debit }; }');
  const e = entry('function second() { const credit = 1; return { credit }; }');
  assert.equal(similarities(d.erased, e.erased).set, 1);
  assert.ok(similarities(d.preserved, e.preserved).set < 1);
});
test('called members remain different, and nested callbacks are not separate candidates', () => {
  const a = entry('function f(x:number[]) { return x.map(y => Math.abs(y)); }');
  const b = entry('function g(x:number[]) { return x.filter(y => Math.abs(y)); }');
  assert.ok(similarities(a.erased, b.erased).set < 1);
  const result = extract('const run = () => items.map(x => x + 1); const helper = () => 1;', 'control.ts');
  assert.deepEqual(result.entries.map(e => e.name), ['run', 'helper']);
  assert.equal(extract('const settings = { defaultValue: () => run() };', 'control.ts').entries.length, 0);
});
test('malformed input and unavailable input are incomplete, corrected input is complete', () => {
  assert.equal(extract('function broken( {', 'bad.ts').entries.length, 0);
  assert.ok(extract('function broken( {', 'bad.ts').diagnostics.length);
  assert.equal(extract('function fixed() { return 1; }', 'fixed.ts').diagnostics.length, 0);
  assert.equal(scan(['/definitely-unavailable/control.ts'], '/').status, 'incomplete');
});
test('multiset notices repetitions that a set discards', () => {
  const a = inventory(['root', ['statement'], ['statement']]);
  const b = inventory(['root', ['statement']]);
  const scores = similarities(a, b);
  assert.ok(scores.multiset !== scores.set);
  assert.ok(scores.weighted < scores.multiset);
});
test('prefix and postfix operators retain different behavior while local names can change', () => {
  const positive = entry('function positive(x:number) { return +x; }');
  const negative = entry('function negative(x:number) { return -x; }');
  const renamed = entry('function renamed(value:number) { return +value; }');
  assert.ok(similarities(positive.preserved, negative.preserved).set < 1);
  assert.equal(similarities(positive.preserved, renamed.preserved).set, 1);
  const increment = entry('function increment(x:number) { x++; return x; }');
  const decrement = entry('function decrement(x:number) { x--; return x; }');
  const copy = entry('function copied(value:number) { value++; return value; }');
  assert.ok(similarities(increment.preserved, decrement.preserved).set < 1);
  assert.equal(similarities(increment.preserved, copy.preserved).set, 1);
});
test('the retained earlier implementation demonstrates the unary regression', async () => {
  const previous = await import('./evidence/run2/measured-implementation/detector.mjs');
  const positive = previous.extract('function positive(x:number) { return +x; }', 'control.ts').entries[0];
  const negative = previous.extract('function negative(x:number) { return -x; }', 'control.ts').entries[0];
  assert.equal(previous.similarities(positive.preserved, negative.preserved).set, 1);
  assert.ok(similarities(entry('function positive(x:number) { return +x; }').preserved,
    entry('function negative(x:number) { return -x; }').preserved).set < 1);
});
