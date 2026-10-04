import * as fs from "node:fs";
import { readMutationRequest, validateMutationResult } from "../../sdk/mutator.ts";

// Run only after the child process exits, using the same assigned request.
const request = readMutationRequest();
const bytes = fs.readFileSync(request.resultPath);
if (bytes.length > request.maxResultBytes) throw new Error("Result exceeds request budget");
const result = validateMutationResult(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)), request);
process.stdout.write(JSON.stringify(result));
