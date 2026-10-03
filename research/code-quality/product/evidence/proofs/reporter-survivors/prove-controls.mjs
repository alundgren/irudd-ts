import { spawnSync } from "node:child_process";
const proof = "/tmp/archguard-quality-product-reporter-survivor-review/test-proof.mjs";
const controls = [
  ["0d5c0d6e796efe2b7968ef30e2fab06faf5aada7a386e1714857fd768587543f", "reporter diagnostics"],
  ["77b9124032d070d5bb793df328917b20a17a3e489efdfeb256ad97254235a662", "Node assertion names"],
  ["e02b02f150dfeff69696ff53954db8d2c25e38f0edda12b2cb2284348dc8b479", "pending runner close"],
  ["b85934e0d092931472b11e8985fc3a9429cfe552d3710776195a2e962994ee03", "pending runner close"],
  ["606b4ce42a192aadf48fc578fc5240f129f311ad5a77fe552768809cc6e311e3", "a second reporter"],
  ["5c47e3ccff87fd25534d67ead3f4264ca9e6f6f34f47e131d86172d7b9c5bb19", "a second reporter"],
];
for(const [id, pattern] of controls) {
  for(const [variant, arguments_] of [["original", []],["final", [pattern]]]) {
    const child = spawnSync(process.execPath,[proof,id,variant,...arguments_],{timeout:95_000,maxBuffer:1_048_576});
    if(child.error || child.status !== 0) throw child.error ?? new Error(child.stderr.toString());
    console.log(child.stdout.toString().trim());
  }
}
