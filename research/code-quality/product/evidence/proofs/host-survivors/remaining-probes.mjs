import fs from "node:fs";
import path from "node:path";
import { stripTypeScriptTypes } from "node:module";

const dir = path.dirname(new URL(import.meta.url).pathname);
const source = fs.readFileSync(path.join(dir, "original/hostClassification.ts"), "utf8");
const resolver = fs.readFileSync(path.join(dir, "original/browserTargetResolver.ts"), "utf8");
const report = JSON.parse(fs.readFileSync("/home/dev/.t3/worktrees/irudd-ts/t3code-dcd18ca5/research/local/code-quality-product/t3-hostClassification-r10.json", "utf8"));
const mutations = new Map(report.results.map((entry) => [entry.mutationId, entry]));
const url = (text, id) => `data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(text)).toString("base64")}#${id}`;
const originalUrl = url(source, "original-additional");
const original = await import(originalUrl);
const changedUrl = (id) => {
  const entry = mutations.get(id);
  return url(source.slice(0, entry.location.start) + entry.replacement + source.slice(entry.location.end), id + "-additional");
};
const rawWitnesses = [
  ["9ed850f996aed2a7fb5c6f6cd16d4107b054038191ac5bfeaf79a72dbb180e34", "::ffff:808:808:ff", "valid-ipv6", "A valid IPv6 address outside the exact IPv4-mapped form is incorrectly treated as public IPv4."],
  ["6049564fa9e9324c819ea4eae520cbebfd9f754bdd5f273bc0cd82aa88ddc356", "::ffff:c000:201", "valid-ipv6", "The low hextet changes mapped 192.0.2.1 into 192.0.192.0, which bypasses the documentation-address exclusion."],
  ["cca81677e7e0d591e9050f2a972d6658387f337609cf184429aa7d4175f36c98", "2001:4860:4860:1:1:1:1:8888", "valid-ipv6", "A valid eight-hextet address without compression loses its head and is rejected."],
  ["b47507887910b2171312eca092a80c593dae0cabf8d8fe4f5636981a2caa0ca1", "2001:4860:4860:0:0:0:8888", "malformed-ipv6", "An uncompressed address with only seven hextets is accepted instead of rejected."],
  ["0709b9e890e67b541eaf16a1f715da0ba346fbd4c0cec9d1df2ccb0236b11504", "2001:4860:4860:1:1:1:1:8888", "valid-ipv6", "Changing the missing-hextet check rejects a valid uncompressed eight-hextet address."],
  ["bf4c512ea3c8a7613ce9083680449ef8b33aeaa6e5a0c41793359fd36108dc50", "2001:4860:4860:1:1:1:1:8888", "valid-ipv6", "Requiring one missing hextet rejects a valid uncompressed eight-hextet address."],
  ["e30724c9edc79d7dd21affc9ca0806b598fdb0e4b569715b4b43432d65185dda", "2001:4860:4860:0:0:0:8888", "malformed-ipv6", "Combining the two invalid-address guards with AND accepts an uncompressed seven-hextet address."],
  ["9c962116fd87cab50d878966d4f529584569087deaded994ca4acf95e9ba0c60", "2001:4860:4860:1:1:1:1:8888", "valid-ipv6", "The compressed-address guard is incorrectly applied to a valid uncompressed eight-hextet address."],
  ["175b8361d6042219f55856af0ee5a7d43386927e122a503dcd5d7c948d0322c7", "2001:4860::4860:1:1:1:8888", "valid-ipv6", "A valid compressed address that omits exactly one zero hextet is rejected."],
  ["bbd242f53b4c26536e2e7a2e117e36d1c20f714c30fe55172420a7893404b872", "2001:4860::4860:0:0:0:0:8888", "malformed-ipv6", "A double colon that omits no hextets is accepted instead of rejected."],
  ["a8164658efcedcd7a3d52caac26080d87353eb5ee9db3cc7be2ac7c74943646e", "2001:4860:4860:0:0:0:8888", "malformed-ipv6", "A parser rejection now returns public, accepting an invalid seven-hextet address."],
  ["f12c22a623e6129364de07fe07d9d64c16a9426d0ea38af9aa75d64cc3a02dc9", "2002::1", "valid-ipv6", "The explicit 2002::/16 exclusion returns public."],
  ["32daaf7159ba0bcddfd0bba7f82307dac261f180b8edfddd8ebc5c9d024a3f62", "2001:200::1", "valid-ipv6", "The 3fff::/20 exclusion also rejects public addresses whose second hextet has zero high bits."],
];
const witnessResults = [];
for (const [id, host, inputKind, reason] of rawWitnesses) {
  const mutant = await import(changedUrl(id));
  const originalValue = original.isPublicFaviconHost(host);
  const mutatedValue = mutant.isPublicFaviconHost(host);
  if (originalValue === mutatedValue) throw new Error(`No witness difference ${id}/${host}`);
  let throughUrl;
  try {
    const parsed = new URL(`http://[${host}]/`);
    const canonical = parsed.hostname;
    const originalCanonicalValue = original.isPublicFaviconHost(canonical);
    const mutatedCanonicalValue = mutant.isPublicFaviconHost(canonical);
    throughUrl = { accepted: true, hostname: canonical, original: originalCanonicalValue, mutated: mutatedCanonicalValue, differs: originalCanonicalValue !== mutatedCanonicalValue };
  } catch (error) {
    throughUrl = { accepted: false, error: String(error), differs: false };
  }
  witnessResults.push({ mutationId: id, host, functionName: "isPublicFaviconHost", inputKind, reason, original: originalValue, mutated: mutatedValue, throughUrl });
}

const resolverFunctionSource = resolver.slice(resolver.indexOf("const readEnvironmentUrl ="));
const consumerIds = [
  "0cf346d7bfac264f0ba3c6b97aa2992c75c4930e72e06f5d9fb0a6378e5d5a59",
  "03d4cd0c51760b5648e6f813523198f070fc79b7006d7c10631bca542c28a6ca",
  "175b42b4fb043a14a73fcea0390a2a5d544fea0784ff36718502274bc802809c",
  "6d6711fa3926715456f1777b5bdaac518b4236c5fba24db224bec00e38211774",
];
const consumerCases = [
  { testName: "supports a local IPv6 environment host", firstLine: 192, base: "http://[::1]:3773", target: { kind: "environment-port", port: 5173 }, expected: "http://localhost:5173/" },
  { testName: "maps local IPv4 environment ports onto localhost for dual-stack guests", firstLine: 203, base: "http://127.0.0.1:3773", target: { kind: "environment-port", port: 5173, path: "/app" }, expected: "http://localhost:5173/app" },
];
const consumerResults = [];
for (const id of [null, ...consumerIds]) {
  const classificationUrl = id ? changedUrl(id) : originalUrl;
  const cases = [];
  for (const test of consumerCases) {
    const prefix = `const {isLocalLoopbackHost,isPrivateNetworkHost}=await import(${JSON.stringify(classificationUrl)});\nconst readPreparedConnection=()=>({httpBaseUrl:${JSON.stringify(test.base)}});\n`;
    const module = await import(url(prefix + resolverFunctionSource, `${id || "baseline"}-${test.base}`));
    let actual;
    try { actual = module.resolveBrowserNavigationTarget("environment-1", test.target).resolvedUrl; }
    catch (error) { actual = { error: String(error) }; }
    if (!id && actual !== test.expected) throw new Error(`Baseline consumer mismatch ${test.testName}`);
    cases.push({ ...test, actual, detected: actual !== test.expected });
  }
  consumerResults.push({ mutationId: id, cases });
}
const implementationId = "8bcf7d215fc1689096c29b9ca7642b28ad0d15e0b5160351b0a8d2e04eacb938";
const implementationMutant = await import(changedUrl(implementationId));
const rawHost = "fc00invalid::1";
const output = {
  nodeVersion: process.version, witnessResults, consumerResults,
  implementationDetail: { mutationId: implementationId, host: rawHost, functionName: "isPrivateNetworkHost", original: original.isPrivateNetworkHost(rawHost), mutated: implementationMutant.isPrivateNetworkHost(rawHost), originalFavicon: original.isPublicFaviconHost(rawHost), mutatedFavicon: implementationMutant.isPublicFaviconHost(rawHost) },
  consumerAdaptation: "Retained the original resolver code from const readEnvironmentUrl onward unchanged. Replaced its imported classifiers with original or single-mutant pure-module imports. Replaced readPreparedConnection with each existing test's fixed connection. Invoked the two existing environment-port test inputs directly. This diagnostic probe is not the original Vitest execution and does not test preview helper imports or other resolver cases.",
};
fs.writeFileSync(path.join(dir, "remaining-probe-results.json"), JSON.stringify(output, null, 2) + "\n");
console.log(JSON.stringify({ rawWitnesses: witnessResults.length, validIpv6: witnessResults.filter((entry) => entry.inputKind === "valid-ipv6").length, malformedIpv6: witnessResults.filter((entry) => entry.inputKind === "malformed-ipv6").length, urlCallerDifferences: witnessResults.filter((entry) => entry.throughUrl.differs).length, consumerDetected: consumerResults.filter((entry) => entry.mutationId && entry.cases.some((test) => test.detected)).length }));
