import { dependencyPaths, runPlugin } from "../sdk/index.ts";
import type { Diagnostic } from "../sdk/index.ts";

runPlugin(project => {
  const diagnostics: Diagnostic[] = [];
  for (const file of project.files) {
    if (!file.path.startsWith("client/")) continue;
    for (const [target, path] of dependencyPaths(project, file.path)) {
      if (target.startsWith("server/")) diagnostics.push({ rule: "client-server", file: file.path, offset: file.imports.find(edge => edge.target === path[1])!.offset, message: `forbidden dependency on ${target}`, evidence: path });
    }
  }
  return diagnostics;
});
