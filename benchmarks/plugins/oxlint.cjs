const fs = require("node:fs");
const path = require("node:path");
const graphs = new Map();

// Oxlint supplies each file's AST. This rule separately parses the generated
// corpus to obtain a graph, so its timing includes that duplicate parse work.
function graph(root) {
  if (graphs.has(root)) return graphs.get(root);
  const ts = require("../toolchain/node_modules/typescript-parser");
  const files = new Map();
  function read(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) read(absolute);
      else if (entry.name.endsWith(".ts")) {
        const relative = path.relative(root, absolute).split(path.sep).join("/");
        const source = ts.createSourceFile(absolute, fs.readFileSync(absolute, "utf8"), ts.ScriptTarget.Latest, true);
        if (source.parseDiagnostics.length) throw new Error(`Invalid benchmark source ${relative}`);
        const edges = [];
        for (const statement of source.statements) {
          if ((ts.isImportDeclaration(statement) || ts.isExportDeclaration(statement)) && statement.moduleSpecifier) {
            const specifier = statement.moduleSpecifier.text;
            if (!specifier.startsWith(".")) throw new Error(`Unsupported benchmark import ${specifier}`);
            const target = path.relative(root, path.resolve(path.dirname(absolute), specifier)).split(path.sep).join("/");
            if (!fs.existsSync(path.join(root, target))) throw new Error(`Missing benchmark target ${target}`);
            edges.push(target);
          }
        }
        files.set(relative, edges);
      }
    }
  }
  for (const directory of ["client", "shared", "server"]) read(path.join(root, directory));
  graphs.set(root, files);
  return files;
}
module.exports = {
  meta: { name: "archguard-benchmark" },
  rules: {
    "direct-import": {
      meta: { type: "problem", schema: [], messages: { forbidden: "Forbidden import" } },
      create(context) {
        return { ImportDeclaration(node) {
          if (node.source.value === "../server/db.ts") context.report({ node, messageId: "forbidden" });
        } };
      }
    },
    "client-server": {
      meta: { type: "problem", schema: [{ type: "object", properties: { root: { type: "string" } }, required: ["root"], additionalProperties: false }], messages: { forbidden: "Forbidden dependency on {{target}}" } },
      create(context) {
        const root = context.options[0].root;
        const file = path.relative(root, context.filename ?? context.getFilename()).split(path.sep).join("/");
        if (!file.startsWith("client/")) return {};
        return { Program(node) {
          const files = graph(root);
          const visited = new Set([file]);
          const queue = [file];
          for (let index = 0; index < queue.length; index++) {
            for (const target of files.get(queue[index]) ?? []) {
              if (visited.has(target)) continue;
              visited.add(target); queue.push(target);
              if (target.startsWith("server/")) context.report({ node, messageId: "forbidden", data: { target } });
            }
          }
        } };
      }
    }
  }
};
