import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { stripTypeScriptTypes } from "node:module";

const dir = path.dirname(new URL(import.meta.url).pathname);
const root = "/home/dev/.t3/worktrees/irudd-ts/t3code-dcd18ca5";
const reportPath = path.join(root, "research/local/code-quality-product/t3-hostClassification-r10.json");
const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const source = fs.readFileSync(path.join(dir, "original/hostClassification.ts"), "utf8");
const shared = fs.readFileSync(path.join(dir, "original/hostClassification.test.ts"), "utf8");
const browser = fs.readFileSync(path.join(dir, "original/browserTargetResolver.test.ts"), "utf8");
const digest = (text) => crypto.createHash("sha256").update(text).digest("hex");
if (digest(source) !== report.selection.selected[0].sha256) throw new Error("Source digest mismatch");
const getModule = async (text, id) => import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(text)).toString("base64")}#${id}`);
const original = await getModule(source, "original");
const tests = [];
const assert = (value, expected, label) => {
  if (JSON.stringify(value) !== JSON.stringify(expected)) throw new Error(`${label}: ${JSON.stringify(value)} != ${JSON.stringify(expected)}`);
};
const strings = (text) => [...text.matchAll(/"(?:[^"\\]|\\.)*"/g)].map((match) => JSON.parse(match[0]));
for (const match of shared.matchAll(/for \(const host of \[([\s\S]*?)\]\) \{\s*expect\(isPublicFaviconHost\(host\), host\)\.toBe\((true|false)\);/g)) {
  for (const host of strings(match[1])) tests.push({ functionName: "isPublicFaviconHost", host, expected: match[2] === "true", origin: "selected-shared-tests" });
}
for (const match of shared.matchAll(/expect\(isPublicFaviconHost\(("(?:[^"\\]|\\.)*")\)\)\.toBe\((true|false)\);/g)) {
  tests.push({ functionName: "isPublicFaviconHost", host: JSON.parse(match[1]), expected: match[2] === "true", origin: "selected-shared-tests" });
}
for (const [name, functionName, expectations] of [
  ["classifies exact private IPv4 and IPv6 boundaries", "isPrivateNetworkHost", [["privateHosts", true], ["publicHosts", false]]],
  ["allows only globally routable hosts to reach a public favicon provider", "isPublicFaviconHost", [["nonPublic", false], ["publicHosts", true]]],
]) {
  const start = browser.indexOf(`it("${name}"`);
  const next = browser.indexOf("\n  it(", start + 1);
  const block = browser.slice(start, next < 0 ? browser.length : next);
  for (const [list, expected] of expectations) {
    const match = block.match(new RegExp(`const ${list} = \\[([\\s\\S]*?)\\];`));
    if (!match) throw new Error(`Missing list ${name}/${list}`);
    for (const host of strings(match[1])) tests.push({ functionName, host, expected, origin: "existing-omitted-browser-test", testName: name });
  }
}
for (const test of tests) assert(original[test.functionName](test.host), test.expected, `Baseline ${test.origin}/${test.host}`);

const hosts = new Set(tests.map((test) => test.host));
const add = (...values) => values.forEach((value) => hosts.add(value));
add("localhost", "::1", "[::1]", "::", "127.0.0.1", "127.127.0.1", "192.127.0.1", "air", "example.com", "example.com..", "example.com...", "test", "alt", "example", "internal", "invalid", "onion", "github.com", "TEST.", "hidden.onion", "", " ");
add("fc00invalid::1", "fd00xyz::1", "fe80-invalid::1", "fc00x:1", "fc00:invalid", "fc00invalid", "fe80q::", "2001:20::", "2001:3::", "2606:4700::");
add("2001:4860:4860:0:0:0:0:8888", "2606:4700:4700:0:0:0:0:1111", "2001:4860:4860:0:0:0:8888", "2001:4860:4860:0:0:0:0:0:8888", "2001:4860:4860:0:0:0:0", "2001:4860:4860", "2001:4860::4860:0:0:0:0:8888", "2001:4860::4860:0:0:0:8888", "2001:1::", "2001:1::1", "2001:1::2", "2001:1::3", "2001:1::4", "2001:1:1::1", "2001:1:1:1:1:1:1:1", "2001:0::1", "2001:2::1", "2001:3::1", "2001:4:112::1", "2001:4:113::1", "2001:20::1", "2001:2f::1", "2001:30::1", "2001:3f::1", "2001:40::1", "2001:100::1", "2001:1ff::1", "2001:200::1", "2002::1", "3fff::1", "3fff:fff::1", "3fff:1000::1", "3ffe::1", "4000::1", "::2", "100::1", "64:ff9b::808:808", "64:ff9b::a00:1", "64:ff9b::c612:1", "64:ff9b:1::808:808", "64:ff9b:0:1::808:808", "64:ff9b:0:0:1::808:808", "64:ff9b:0:0:0:1:808:808", "::ffff:808:808:ff", "::ffff:808", "::ffff:0:0:808:808", "::ffff:a00:808", "::ffff:808:a00", "::ffff:c0a8:808", "::ffff:c612:808", "::ffff:0:808", "::ffff:ff:0", "::ffff:0:gg", "::ffff:gg:808", "::ffff:8.8.8.8", "::ffff:198.18.0.1", "foo:bar", "2001:zz::1");
for (const a of [0, 1, 10, 11, 100, 126, 127, 128, 169, 170, 172, 192, 198, 203, 223, 224, 225, 254, 255]) {
  for (const b of [0, 1, 2, 10, 15, 16, 17, 18, 19, 20, 31, 32, 51, 63, 64, 88, 99, 100, 113, 127, 128, 168, 169, 192, 254, 255]) {
    for (const c of [0, 1, 2, 99, 100, 113, 254]) add(`${a}.${b}.${c}.1`);
  }
}
for (const a of [0, 0x64, 0x100, 0x2000, 0x2001, 0x2002, 0x2606, 0x3ffe, 0x3fff, 0x4000, 0x5f00, 0xfbff, 0xfc00, 0xfdff, 0xfe80, 0xfebf, 0xfec0, 0xffff]) {
  for (const b of [0, 1, 2, 3, 4, 5, 0x1f, 0x20, 0x2f, 0x30, 0x3f, 0x40, 0xff, 0x100, 0x1ff, 0x200, 0xdb8, 0xfff, 0x1000, 0xff9b, 0xffff]) {
    add(`${a.toString(16)}:${b.toString(16)}::1`, `${a.toString(16)}:${b.toString(16)}:0:0:0:0:0:1`);
  }
}
for (const suffix of ["808:808", "a00:1", "7f00:1", "c000:201", "c612:1", "c613:ffff", "e000:1", "cb00:7101"]) {
  add(`64:ff9b::${suffix}`, `::ffff:${suffix}`, `64:ff9b:0:0:0:0:${suffix}`);
}
const functions = ["isPublicFaviconHost", "isPrivateNetworkHost", "isLocalLoopbackHost", "normalizeHostname"];
const corpus = [...hosts];
const values = new Map(functions.map((name) => [name, corpus.map((host) => original[name](host))]));
const results = [];
for (const mutant of report.results.filter((entry) => entry.outcome === "survived")) {
  const { start, end } = mutant.location;
  if (source.slice(start, end) !== mutant.expected) throw new Error(`Wrong span ${mutant.mutationId}`);
  const changed = source.slice(0, start) + mutant.replacement + source.slice(end);
  const module = await getModule(changed, mutant.mutationId);
  const existingFailures = tests.filter((test) => test.origin === "existing-omitted-browser-test").flatMap((test) => {
    try {
      const actual = module[test.functionName](test.host);
      return actual === test.expected ? [] : [{ ...test, actual }];
    } catch (error) { return [{ ...test, error: String(error) }]; }
  });
  const sharedFailures = tests.filter((test) => test.origin === "selected-shared-tests").flatMap((test) => {
    try { const actual = module[test.functionName](test.host); return actual === test.expected ? [] : [{ ...test, actual }]; }
    catch (error) { return [{ ...test, error: String(error) }]; }
  });
  const differences = {};
  for (const name of functions) {
    const witnesses = [];
    let count = 0;
    for (let index = 0; index < corpus.length; index += 1) {
      const host = corpus[index];
      const expected = values.get(name)[index];
      let actual;
      try { actual = module[name](host); }
      catch (error) { actual = { error: String(error) }; }
      if (JSON.stringify(actual) !== JSON.stringify(expected)) {
        count += 1;
        if (witnesses.length < 5) witnesses.push({ host, original: expected, mutated: actual });
      }
    }
    differences[name] = { count, witnesses };
  }
  results.push({ mutationId: mutant.mutationId, existingFailures, sharedFailures, differences });
}
const output = {
  sourceSha256: digest(source), reportSha256: digest(fs.readFileSync(reportPath)),
  nodeVersion: process.version, testCaseCount: tests.length,
  sharedCaseCount: tests.filter((test) => test.origin === "selected-shared-tests").length,
  omittedExistingCaseCount: tests.filter((test) => test.origin === "existing-omitted-browser-test").length,
  corpusCount: corpus.length, functionNames: functions, results,
  method: "Each original survivor was edited independently in memory, stripped with Node24 node:module stripTypeScriptTypes, imported as a pure module, and evaluated directly. Existing assertions were extracted from the frozen test lists and checked against their expected booleans. Authored corpus comparisons use original module outputs as a reference, not an independent specification. This is a bounded diagnostic probe and does not replace a Vitest mutation replay. Zero sampled differences do not prove equivalence.",
};
fs.writeFileSync(path.join(dir, "probe-results.json"), JSON.stringify(output, null, 2) + "\n");
fs.writeFileSync(path.join(dir, "probe-cases.json"), JSON.stringify({tests, corpus}, null, 2) + "\n");
console.log(JSON.stringify({ survivorCount: results.length, corpusCount: corpus.length, testCaseCount: tests.length,
  omittedExistingDetect: results.filter((entry) => entry.existingFailures.length).length,
  sharedUnexpectedDifferences: results.filter((entry) => entry.sharedFailures.length).length,
  exportedDifference: results.filter((entry) => Object.values(entry.differences).some((value) => value.count)).length,
  noExportedSampleDifference: results.filter((entry) => Object.values(entry.differences).every((value) => !value.count)).length,
}));
