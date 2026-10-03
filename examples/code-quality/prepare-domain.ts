import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const outputArgument = process.argv[2];
if (!outputArgument || process.argv.length !== 3) throw new Error("Usage: node examples/code-quality/prepare-domain.ts <new output directory>");
const output = path.resolve(outputArgument);
if (fs.existsSync(output)) throw new Error("Output directory must be new");
fs.mkdirSync(output, { recursive: true });
for (const mode of ["weak", "strong"]) {
  const source = fileURLToPath(new URL(`./mutator-${mode}.json`, import.meta.url));
  const config = JSON.parse(fs.readFileSync(source, "utf8"));
  // Pin the running Node executable so version-manager dispatchers do not need
  // the original user's home directory inside an isolated worker.
  config.execution.command[0] = process.execPath;
  fs.writeFileSync(path.join(output, `${mode}.json`), JSON.stringify(config, null, 2) + "\n");
}
console.log(`Profiles written to ${output}; use the Archguard repository as --root.`);
