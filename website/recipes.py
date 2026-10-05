"""Maintained cookbook content. Configs and source examples also drive CLI checks."""
import json

CATEGORIES = {
    "start": ("Start here", "Build the CLI, select your files, and read a result."),
    "dependencies": ("Control dependencies", "Keep imports, packages, and calls where they belong."),
    "conventions": ("Keep file conventions", "Check exports, service modules, and companion files."),
    "extensions": ("Add your own checks", "Use graph facts or opt into compiler member facts."),
    "review": ("Review code and tests", "Inspect duplicate code and test behavior with mutations."),
}
RECIPES = []


def block(label, language, code):
    return {"label": label, "language": language, "code": code.strip()}


def json_block(label, value):
    return block(label, "json", json.dumps(value, indent=2))


def add(slug, title, category, summary, capabilities, nodes, result, boundary,
        blocks, references, related=(), needs="Built Archguard CLI on your PATH.", fixture=None):
    RECIPES.append(dict(slug=slug, title=title, category=category, summary=summary,
                        capabilities=capabilities, nodes=nodes, result=result,
                        boundary=boundary, blocks=blocks, references=references,
                        related=list(related), needs=needs, fixture=fixture))


def policy(slug, title, category, summary, rule, before, fixed, control, nodes,
           boundary, *, include=None, extra=None, repository=False, related=()):
    config = {"schemaVersion": 1, "include": include or ["src/**/*.ts"]}
    if extra:
        config.update(extra)
    if repository:
        config["repository"]["rules"] = [rule]
    else:
        config["rules"] = [rule]
    changed = [p for p in fixed if before.get(p) != fixed[p]]
    removed = [p for p in before if p not in fixed]
    blocks = [json_block("archguard.json", config)]
    # Show every input so the miniature project can be reproduced from the page.
    for path, source in before.items():
        blocks.append(block(path + " · before", "json" if path.endswith(".json") else "typescript", source))
    for path in changed:
        blocks.append(block(path + " · correction", "json" if path.endswith(".json") else "typescript", fixed[path]))
    for path in removed:
        blocks.append(block("Correction", "shell", "rm " + path))
    blocks.append(block("Run from this project directory", "shell", "archguard check --root . --config archguard.json --json"))
    ref = "docs/guides/repository-rules.md" if repository else "docs/guides/rules.md"
    add(slug, title, category, summary, ["repository." + rule["kind"] if repository else rule["kind"]],
        nodes, f"Before: exit 1, finding {rule['id']}. After the correction: exit 0.", boundary,
        blocks, [ref], related,
        fixture=dict(config=config, before=before, fixed=fixed, control=control, rule=rule["id"]))


add("first-check", "Run your first architecture check", "start",
    "Build Archguard and stop a client module from importing server code.", ["check", "installation"],
    [("Source files", "neutral"), ("Your rules", "neutral"), ("Findings + exit code", "info")],
    "The client import below produces exit 1. Remove that import for exit 0.",
    "Exit 2 means analysis did not complete or configuration is invalid. Source resolution does not check compiler types.",
    [block("In the Archguard checkout · Rust 1.96+", "shell", "cargo build --release --locked\nexport PATH=\"$PWD/target/release:$PATH\""),
     json_block("In your project · archguard.json", {"schemaVersion": 1, "include": ["src/**/*.ts", "src/**/*.tsx"], "rules": [{"id": "client-server", "kind": "forbiddenDependency", "files": ["src/client/**"], "targets": ["src/server/**"], "transitive": True}]}),
     block("src/server/db.ts", "typescript", "export const db = {};"),
     block("src/client/main.ts", "typescript", "import { db } from '../server/db';\nconsole.log(db);"),
     block("src/client/main.ts · correction", "typescript", "export {};"),
     block("In your project", "shell", "archguard check --root . --config archguard.json\narchguard check --root . --config archguard.json --json")],
    ["README.md", "docs/guides/configuration.md"], ["block-server", "read-results"], needs="Rust 1.96+ to build. No Node installation needed for graph checks.")

add("select-sources", "Choose the files Archguard analyzes", "start",
    "Include application sources and their local dependencies. Leave generated files out.", ["include", "exclude", "packageManifests"],
    [("Selected sources", "info"), ("Resolved imports", "neutral"), ("Policy checks", "neutral")],
    "App and shared TypeScript sources enter the graph. Generated sources do not.",
    "An import into excluded source makes analysis incomplete. Rule and role selectors never add files to discovery.",
    [json_block("archguard.json", {"schemaVersion": 1, "include": ["apps/**/*.ts", "packages/**/*.ts"], "exclude": ["**/generated/**"], "packageManifests": ["package.json", "apps/*/package.json", "packages/*/package.json"], "rules": []}),
     block("Run from your project root", "shell", "archguard facts --root . --config archguard.json > project-facts.json")],
    ["docs/guides/configuration.md"], ["resolve-imports", "export-graph"])

add("resolve-imports", "Resolve TypeScript aliases and package imports", "start",
    "Use tsconfig paths, workspace package exports, and explicit source lookup settings.", ["conditions", "extensions", "extensionAliases", "requireExternalResolution"],
    [("@app/domain", "info"), ("tsconfig / exports", "neutral"), ("Source target", "neutral")],
    "Archguard reads nearest and inherited tsconfigs. Required external lookups fail with exit 2 when packages are missing.",
    "Keep requireExternalResolution false for uninstalled external packages. Source lookup is separate from compiler and runtime resolution.",
    [json_block("tsconfig.json", {"compilerOptions": {"baseUrl": ".", "paths": {"@app/*": ["src/*"]}}}),
     json_block("archguard.json", {"schemaVersion": 1, "include": ["src/**/*.ts"], "conditions": ["types", "import", "default"], "extensions": [".ts", ".tsx", ".js", ".json"], "extensionAliases": [[".js", [".ts", ".tsx", ".js"]]], "requireExternalResolution": True}),
     block("After installing your project's packages", "shell", "archguard check --root . --config archguard.json --json")],
    ["docs/guides/configuration.md"], ["select-sources", "public-entry"])

add("read-results", "Read clean, failing, and incomplete results", "start",
    "Use completeness and the process exit code to decide the next action.", ["exitCodes", "json"],
    [("0 · clean", "ok"), ("1 · violations", "danger"), ("2 · incomplete", "warn")],
    "0: all checks completed and passed. 1: all checks completed with violations. 2: fix config, parsing, resolution, or required facts.",
    "An incomplete report can still contain useful findings. Dryer and mutation runs exit 0 for complete evidence, including matches or survivors.",
    [block("Capture a check report", "shell", "archguard check --root . --config archguard.json --json > archguard-report.json"),
     block("Inspect completeness and findings with Node 24", "javascript", "import { readFileSync } from 'node:fs';\nconst report = JSON.parse(readFileSync('archguard-report.json', 'utf8'));\nconsole.log(report.complete, report.diagnostics, report.problems);")],
    ["docs/guides/configuration.md", "docs/code-quality.md"], ["first-check", "plan-mutations"])

policy("block-server", "Keep server code out of the client", "dependencies",
       "Catch direct imports and indirect paths through shared modules.",
       dict(id="client-server", kind="forbiddenDependency", files=["src/client/**"], targets=["src/server/**"], transitive=True, includeTypes=False),
       {"src/client/main.ts": "import '../shared/data';", "src/shared/data.ts": "import '../server/db';", "src/server/db.ts": "export const db = {};"},
       {"src/client/main.ts": "export {};", "src/shared/data.ts": "import '../server/db';", "src/server/db.ts": "export const db = {};"},
       {"src/client/main.ts": "import type { Db } from '../server/db';", "src/server/db.ts": "export type Db = {};"},
       [("client/main.ts", "info"), ("shared/data.ts", "neutral"), ("server/db.ts · blocked", "danger")],
       "This checks resolved module reachability. includeTypes: false ignores erased type imports. It does not prove code executes.", related=["block-imports", "role-dependencies"])

policy("block-imports", "Ban an import by its written name", "dependencies",
       "Keep Node builtins out of browser sources, even when they are external endpoints.",
       dict(id="browser-no-fs", kind="forbiddenImport", files=["src/client/**"], specifiers=["node:fs", "node:fs/*"]),
       {"src/client/main.ts": "import { readFileSync } from 'node:fs';"},
       {"src/client/main.ts": "export const load = () => fetch('/settings.json');"},
       {"src/client/main.ts": "import { basename } from 'node:path';"},
       [("Browser source", "info"), ("node:fs · blocked", "danger")],
       "Specifiers match the written import string. Use forbiddenDependency for resolved file paths.", related=["block-server", "block-calls"])

policy("block-calls", "Keep runtime calls at the application edge", "dependencies",
       "Report modules that call Effect.runPromise, and modules that depend on them.",
       dict(id="domain-no-run", kind="forbiddenCall", files=["src/domain/**"], origins=["effect/Effect#runPromise"], transitive=True),
       {"src/domain/orders.ts": "import '../runner';", "src/runner.ts": "import * as E from 'effect/Effect';\nE.runPromise(task);"},
       {"src/domain/orders.ts": "export const order = 1;", "src/runner.ts": "import * as E from 'effect/Effect';\nE.runPromise(task);"},
       {"src/domain/orders.ts": "import * as E from 'effect/Effect';\nfunction run(E: any) { E.runPromise(task); }"},
       [("domain/orders.ts", "info"), ("runner.ts", "neutral"), ("runPromise · blocked", "danger")],
       "Imported aliases are recognized; locally shadowed names are not attributed to the import. A dependency path does not prove a callback runs.", related=["block-server"])

policy("no-cycles", "Find dependency cycles", "dependencies",
       "Catch two modules that import each other before the cycle grows.",
       dict(id="no-cycles", kind="noCycles", files=["src/**"], includeTypes=False),
       {"src/a.ts": "import './b';\nexport const a = 1;", "src/b.ts": "import './a';\nexport const b = 2;"},
       {"src/a.ts": "import './b';\nexport const a = 1;", "src/b.ts": "export const b = 2;"},
       {"src/a.ts": "import('./b');\nexport const a = 1;", "src/b.ts": "import './a';\nexport const b = 2;"},
       [("a.ts → b.ts", "neutral"), ("b.ts → a.ts · cycle", "danger")],
       "noCycles omits literal dynamic-import edges. includeTypes: false also omits type-only edges.", related=["block-server"])

policy("package-dependencies", "Keep a package free of a dependency", "dependencies",
       "Stop a domain workspace package from declaring React.",
       dict(id="domain-no-react", kind="packageDependency", files=["packages/domain/package.json"], specifiers=["react", "react-dom"]),
       {"packages/domain/package.json": '{"name":"@app/domain","dependencies":{"react":"*"}}', "src/main.ts": "export {};"},
       {"packages/domain/package.json": '{"name":"@app/domain","dependencies":{}}', "src/main.ts": "export {};"},
       {"packages/domain/package.json": '{"name":"@app/domain","devDependencies":{"react":"*"}}', "src/main.ts": "export {};"},
       [("@app/domain", "neutral"), ("react dependency · blocked", "danger")],
       "This checks production, peer, and optional dependencies. Development dependencies are outside the current package facts.", related=["public-entry"])

policy("public-entry", "Require imports through a public package entry", "dependencies",
       "Stop consumers from reaching into a workspace package's private source paths.",
       dict(id="api-public-entry", kind="publicEntry", files=["src/**"], targets=["packages/api/src/**"], specifiers=["@app/api"]),
       {"packages/api/package.json": '{"name":"@app/api","exports":{".":"./src/index.ts"}}', "packages/api/src/index.ts": "export const api = 1;", "src/main.ts": "import '../packages/api/src/index';"},
       {"packages/api/package.json": '{"name":"@app/api","exports":{".":"./src/index.ts"}}', "packages/api/src/index.ts": "export const api = 1;", "src/main.ts": "import '@app/api';"},
       {"src/main.ts": "import './helper';", "src/helper.ts": "export {};"},
       [("Consumer", "info"), ("@app/api · allowed", "ok"), ("Private path · blocked", "danger")],
       "Include protected package sources in discovery. The allowed list checks the written specifier for imports that resolve into those sources.",
       include=["src/**/*.ts", "packages/**/*.ts"], related=["resolve-imports", "required-exports"])

policy("required-exports", "Require a named export", "conventions",
       "Make every adapter expose a create function.",
       dict(id="adapter-create", kind="requiredExport", files=["src/adapters/*.ts"], names=["create"]),
       {"src/adapters/mail.ts": "export const send = () => {};"},
       {"src/adapters/mail.ts": "export const create = () => ({});"},
       {"src/adapters/mail.ts": "export { create } from '../factory';", "src/factory.ts": "export const create = () => ({});"},
       [("mail.ts", "neutral"), ("create export · required", "ok")],
       "Each name needs one visible export origin. This checks presence, not the function signature; external star exports are not enumerated.", related=["required-sources", "service-layer"])

policy("required-sources", "Require an analyzed source file", "conventions",
       "Require a contracts entry point to exist in the selected source inventory.",
       dict(id="contracts-entry", kind="requiredFile", files=["src/contracts/index.ts"]),
       {"src/main.ts": "export {};"},
       {"src/main.ts": "export {};", "src/contracts/index.ts": "export type Message = { id: string };"},
       {"src/main.ts": "export {};", "src/contracts/index.ts": "export {};", "notes.md": "Outside the source inventory."},
       [("Selected sources", "neutral"), ("contracts/index.ts · required", "ok")],
       "Every configured pattern must match an analyzed source. This cannot require a README or another unsupported non-source file.", related=["select-sources", "companions"])

service = "import * as C from 'effect/Context';\nexport class Orders extends C.Service<Orders, {}>()('app/Orders') {}"
policy("service-namespace", "Use namespace imports for Effect services", "conventions",
       "Keep a service and its layer under one module name at call sites.",
       dict(id="service-namespace", kind="serviceNamespace", files=["src/**"]),
       {"src/orders.ts": service + "\nexport const layer = 1;", "src/main.ts": "import { Orders, layer } from './orders';"},
       {"src/orders.ts": service + "\nexport const layer = 1;", "src/main.ts": "import * as Orders from './orders';"},
       {"src/orders.ts": service, "src/main.ts": "import type { Orders } from './orders';"},
       [("Consumer", "info"), ("import * as Orders", "ok"), ("Service module", "neutral")],
       "Only recognized Effect service syntax and value consumers are checked. This is an import convention.", related=["unique-service-id", "service-layer"])

policy("unique-service-id", "Catch duplicate Effect service identifiers", "conventions",
       "Give each recognized service its own literal identifier.",
       dict(id="unique-service-id", kind="uniqueServiceId", files=["src/**"]),
       {"src/orders.ts": service, "src/payments.ts": "import * as C from 'effect/Context';\nexport class Payments extends C.Service<Payments, {}>()('app/Orders') {}"},
       {"src/orders.ts": service, "src/payments.ts": "import * as C from 'effect/Context';\nexport class Payments extends C.Service<Payments, {}>()('app/Payments') {}"},
       {"src/orders.ts": service, "src/payments.ts": "export const label = 'app/Orders';"},
       [("Orders · app/Orders", "neutral"), ("Payments · same ID", "danger")],
       "Computed identifiers and arbitrary service factories are outside the recognized syntax.", related=["service-namespace"])

policy("service-layer", "Require an Effect service layer export", "conventions",
       "Require one visible value export called layer on service files.",
       dict(id="service-layer", kind="serviceLayer", files=["src/**"]),
       {"src/orders.ts": service},
       {"src/orders.ts": service + "\nexport const layer = 1;"},
       {"src/helper.ts": "export const helper = 1;"},
       [("Recognized service", "neutral"), ("layer value export · required", "ok")],
       "The placeholder value demonstrates export presence only. This rule does not prove that layer constructs the service.", related=["service-namespace", "companions"])

policy("classify-files", "Give every source file exactly one role", "conventions",
       "Catch sources that fall outside your conventions or match overlapping roles.",
       dict(id="classify-source", kind="classified", files=["src/**"]),
       {"src/services/orders.ts": "export {};", "src/misc.ts": "export {};"},
       {"src/services/orders.ts": "export {};", "src/contracts/message.ts": "export {};"},
       {"src/services/orders.ts": "export {};", "src/contracts/message.ts": "export {};", "src/main.test.ts": "export {};"},
       [("services/*.ts", "info"), ("contracts/*.ts", "neutral"), ("Unclassified · finding", "danger")],
       "Roles filter selected sources. A classification rule chooses where exactly-one membership is required.",
       extra={"repository": {"roles": [{"id": "service", "files": ["src/services/**"]}, {"id": "contract", "files": ["src/contracts/**"]}, {"id": "test", "files": ["src/*.test.ts"]}]}},
       repository=True, related=["companions", "role-dependencies"])

policy("companions", "Require a matching companion file", "conventions",
       "Require src/layers/Orders.ts whenever src/services/Orders.ts is analyzed.",
       dict(id="service-companion", kind="companion", role="service", replace=["/services/", "/layers/"]),
       {"src/services/Orders.ts": "export class Orders {}"},
       {"src/services/Orders.ts": "export class Orders {}", "src/layers/Orders.ts": "export const live = 1;"},
       {"src/services/Orders.test.ts": "export {};"},
       [("services/Orders.ts", "info"), ("layers/Orders.ts · required", "ok")],
       "The replacement string must occur exactly once. The companion must be analyzed; presence does not prove it implements the service.",
       extra={"repository": {"roles": [{"id": "service", "files": ["src/services/*.ts"], "exclude": ["**/*.test.ts"]}]}},
       repository=True, related=["classify-files", "registry-imports"])

policy("registry-imports", "Require a registry to import every migration", "conventions",
       "Catch a new migration with no static value import in the registry.",
       dict(id="migration-import", kind="registryImport", role="migration", registry="src/migrations.ts"),
       {"src/migrations/001.ts": "export default 1;", "src/migrations.ts": "export const migrations = [];"},
       {"src/migrations/001.ts": "export default 1;", "src/migrations.ts": "import first from './migrations/001';\nexport const migrations = [first];"},
       {"src/migrations/001.test.ts": "export {};", "src/migrations.ts": "export const migrations = [];"},
       [("migrations.ts", "info"), ("Static value import", "neutral"), ("001.ts · required", "ok")],
       "Dynamic imports, require calls, type imports, and re-exports do not qualify. This checks imports, not registry membership or execution.",
       extra={"repository": {"roles": [{"id": "migration", "files": ["src/migrations/*.ts"], "exclude": ["**/*.test.ts"]}]}},
       repository=True, related=["companions"])

policy("role-dependencies", "Block dependencies between file roles", "conventions",
       "Keep contracts independent of host implementation code.",
       dict(id="contracts-no-host", kind="forbiddenDependency", **{"from": "contract", "to": "host"}, transitive=True, includeTypes=True),
       {"src/contracts/message.ts": "import '../host/main';", "src/host/main.ts": "export const host = 1;"},
       {"src/contracts/message.ts": "export type Message = { id: string };", "src/host/main.ts": "export const host = 1;"},
       {"src/contracts/message.ts": "export type Message = { id: string };", "src/host/main.ts": "import type { Message } from '../contracts/message';"},
       [("Contract role", "info"), ("Host role · blocked", "danger")],
       "Role exclusions affect source and target membership. Transitive paths can pass through modules with no role.",
       extra={"repository": {"roles": [{"id": "contract", "files": ["src/contracts/**"]}, {"id": "host", "files": ["src/host/**"]}]}},
       repository=True, related=["block-server", "classify-files"])

add("rust-sources", "Check direct Rust module dependencies", "dependencies",
    "Apply a dependency rule to crate::module imports in Rust source.", ["rust"],
    [("client.rs", "info"), ("crate::server", "neutral"), ("server.rs · blocked", "danger")],
    "The direct crate::server import produces a client-server finding. Remove it for a clean check.",
    "Only the first module component resolves. self::, super::, inline modules, nested resolution, macros, and compiler types are unsupported.",
    [json_block("archguard.json", {"schemaVersion": 1, "include": ["src/**/*.rs"], "rules": [{"id": "client-server", "kind": "forbiddenDependency", "files": ["src/client.rs"], "targets": ["src/server.rs"]}]}),
     block("src/lib.rs", "rust", "pub mod client;\npub mod server;"),
     block("src/client.rs · before", "rust", "use crate::server;\npub fn run() {}"),
     block("src/server.rs", "rust", "pub fn serve() {}"),
     block("src/client.rs · correction", "rust", "pub fn run() {}"),
     block("Run from your project", "shell", "archguard check --root . --config archguard.json --json")],
    ["docs/guides/configuration.md"], ["block-server"])

add("export-graph", "Export the source dependency graph", "extensions",
    "Feed versioned project facts into your own reports or graph tools.", ["facts", "ProjectFacts"],
    [("Selected source", "neutral"), ("ProjectFacts v1", "info"), ("Your report", "neutral")],
    "project-facts.json contains sources, import resolution states, exports, recognized calls and services, and package facts.",
    "Keep every import's resolution status and analysis problems. A selected dependency closure does not establish whole-repository coverage.",
    [block("In your project", "shell", "archguard facts --root . --config archguard.json > project-facts.json"),
     block("Node 24 · inspect-graph.mjs", "javascript", "import { readFileSync } from 'node:fs';\nconst project = JSON.parse(readFileSync('project-facts.json', 'utf8'));\nfor (const file of project.files) {\n  console.log(file.path, file.imports);\n}")],
    ["sdk/README.md", "src/facts.rs"], ["typescript-plugin", "compiler-facts"])

add("typescript-plugin", "Add a TypeScript graph plugin", "extensions",
    "Run a custom rule over the same graph as built-in policies.", ["plugins", "TypeScript SDK", "createDependencyQuery"],
    [("ProjectFacts v1", "info"), ("Explicit Node command", "neutral"), ("Custom findings", "neutral")],
    "The bundled project is clean: exit 0 with no findings. Apply the shared/message.ts edit below and rerun to report client → shared → server, exit 1. Restore the file afterward.",
    "Plugins are trusted commands, run from the configuration directory. Reuse one dependency query for many roots. Process limits are not a sandbox.",
    [block("Archguard checkout · Node 24", "shell", "cargo build --locked --bins --examples\n./target/debug/archguard check --root examples/graph-plugin --config examples/graph-plugin/archguard.json --json"),
     block("examples/graph-plugin/shared/message.ts · try a violation", "typescript", "import '../server/db.ts';\nexport const message = 'hello';"),
     json_block("Minimal plugin registration · archguard.json", {"schemaVersion": 1, "plugins": [{"name": "team-rules", "command": ["node", "./team-rules.ts"], "timeoutMs": 5000}]}),
     block("team-rules.ts · SDK copied to ./sdk", "typescript", "import { createDependencyQuery, runPlugin } from './sdk/index.ts';\n\nrunPlugin(project => {\n  const dependencies = createDependencyQuery(project);\n  return project.files.flatMap(file => {\n    if (!file.path.startsWith('src/client/')) return [];\n    return [...dependencies(file.path)]\n      .filter(([target]) => target.startsWith('src/server/'))\n      .map(([target, path]) => ({\n        rule: 'client-server', file: file.path, offset: 0,\n        message: `Client reaches ${target}`, evidence: path\n      }));\n  });\n});")],
    ["docs/extensions/plugins.md", "examples/graph-plugin/plugin.ts", "sdk/index.ts"], ["rust-plugin", "export-graph"], needs="Archguard CLI, Node 24, and the complete sdk/ directory for the custom example. Unix for subprocess plugins.")

add("rust-plugin", "Use the Rust graph-rule API", "extensions",
    "Check immutable project facts in Rust or return findings from an executable plugin.", ["Rust SDK", "ProjectRule"],
    [("ProjectFacts v1", "info"), ("ProjectRule::check", "neutral"), ("Diagnostics", "neutral")],
    "The bundled graph is clean: diagnostics is empty. Apply the shared/message.ts edit below, then export facts again and rerun the plugin to see the client-server finding. Restore the file afterward.",
    "Executable plugins read one JSON project on stdin and write one versioned diagnostics object on stdout. Put logs on stderr.",
    [block("Archguard checkout", "shell", "cargo build --locked --bins --examples\n./target/debug/archguard facts --root examples/graph-plugin --config examples/graph-plugin/archguard.json > /tmp/graph.json\n./target/debug/examples/graph_plugin < /tmp/graph.json"),
     block("examples/graph-plugin/shared/message.ts · try a violation", "typescript", "import '../server/db.ts';\nexport const message = 'hello';"),
     block("Core API · excerpt from examples/graph-plugin/main.rs", "rust", "use archguard::facts::ProjectRule;\n\nlet diagnostics = rule.check(&project)?;\nprintln!(\"{}\", serde_json::json!({\n    \"schemaVersion\": 1,\n    \"diagnostics\": diagnostics\n}));")],
    ["examples/graph-plugin/main.rs", "docs/extensions/plugins.md"], ["typescript-plugin"], needs="Rust 1.96+ to build the repository example. The Rust snippet is an API excerpt; use the linked complete main.rs.")

add("missing-member", "Check an inferred public member", "extensions",
    "Opt into the pinned compiler provider and catch account.missing on an inferred object.", ["missingMember", "semantic-config"],
    [("account.missing", "neutral"), ("Compiler member facts", "info"), ("Missing member", "danger")],
    "account.name passes. account.missing yields a member finding and compiler error, exit 2. With @ts-ignore suppressing that error, the rule can still yield exit 1.",
    "The provider pins TypeScript 7.0.2 and its experimental API. Unknown, any, error, and unavailable states do not establish a clean result.",
    [block("Archguard checkout · Node 24", "shell", "npm ci --prefix providers/typescript7 --ignore-scripts --no-audit --no-fund\ncargo build --locked --bins --examples\n./target/debug/archguard check --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json --json"),
     block("examples/semantic/main.ts · try a missing member", "typescript", "const account = { name: 'Ada' };\naccount.missing;"),
     json_block("examples/semantic/semantic.json", {"schemaVersion": 1, "provider": {"name": "typescript7", "command": ["node", "../../providers/typescript7/provider.mjs"], "timeoutMs": 30000}, "contexts": [{"id": "app", "tsconfig": "tsconfig.json", "files": ["main.ts"]}], "rules": [{"id": "missing-member", "kind": "missingMember", "files": ["**"]}]})],
    ["examples/semantic/README.md", "docs/extensions/semantic-provider.md"], ["compiler-facts", "read-results"], needs="Archguard checkout, Node 24, optional provider install, and Unix. Restore main.ts after trying the change.")

add("compiler-facts", "Export compiler facts by context", "extensions",
    "Inspect inferred receiver types and public member status separately from source graph facts.", ["semantic-facts", "SemanticFacts", "SemanticRule"],
    [("Explicit tsconfig", "neutral"), ("SemanticFacts v1", "info"), ("Member status", "neutral")],
    "The export includes context-specific dot-access facts, member states, compiler diagnostics, and completion problems.",
    "Computed and private properties are outside this contract. Public optional access and JSX member tags are included. Context selectors never add sources.",
    [block("Archguard checkout · after the provider setup", "shell", "./target/debug/archguard semantic-facts --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json > /tmp/semantic-facts.json"),
     block("inspect-semantic.ts · read host-validated facts from stdin", "typescript", "import { readSemanticFacts, missingMemberDiagnostics } from './sdk/index.ts';\n\nconst facts = readSemanticFacts();\nconsole.log(missingMemberDiagnostics(facts, 'missing-member'));"),
     block("Archguard checkout · Node 24", "shell", "node inspect-semantic.ts < /tmp/semantic-facts.json")],
    ["docs/extensions/semantic-provider.md", "sdk/semantic.ts"], ["missing-member", "export-graph"], needs="Complete the inferred-member recipe first. Run the SDK snippet from the Archguard checkout.")

add("graph-cache", "Reuse a validated source graph", "extensions",
    "Keep a complete graph between checks while still rerunning policies and plugins.", ["cache"],
    [("Revalidate inputs", "neutral"), ("Reuse complete graph", "info"), ("Rerun every policy", "neutral")],
    "The first complete check writes the cache. An unchanged second check can reuse syntax and resolved imports.",
    "Keep the cache outside selected inputs. Compiler facts rerun independently. Recorded cache measurements were slower; reuse counts do not prove a speedup.",
    [block("In your project · run twice", "shell", "archguard check --root . --config archguard.json --cache /tmp/project.archguard-cache.json --json\narchguard check --root . --config archguard.json --cache /tmp/project.archguard-cache.json --json")],
    ["docs/guides/cache.md"], ["export-graph", "missing-member"])

add("find-duplicates", "Find structurally similar functions", "review",
    "Compare renamed invoice and order logic before deciding what to extract.", ["dryer", "similarityThreshold"],
    [("Selected functions", "neutral"), ("Normalized comparison", "info"), ("Pairs to inspect", "neutral")],
    "The authored example reports similar function pairs with locations and similarity evidence. A complete report exits 0 even with matches.",
    "Dryer supports TypeScript and TSX. Similarity is review evidence, not proof that two functions should be combined.",
    [block("Archguard checkout", "shell", "./target/debug/archguard dryer --root . --config examples/code-quality/dryer.json --json"),
     json_block("Your project · dryer.json", {"schemaVersion": 1, "selection": {"include": ["src/**/*.ts", "src/**/*.tsx"]}, "minimumLines": 4, "minimumNodes": 20, "similarityThreshold": 0.82}),
     block("Your project", "shell", "archguard dryer --root . --config dryer.json --json > duplicate-report.json")],
    ["docs/code-quality.md", "examples/code-quality/clones.ts"], ["tune-duplicates", "duplicate-groups"])

add("tune-duplicates", "Compare duplicate normalization settings", "review",
    "Keep literal values for a stricter comparison, or erase local names when investigating shifted declarations.", ["normalization", "dryer cache"],
    [("Function source", "neutral"), ("Explicit normalization", "info"), ("Compare both reports", "neutral")],
    "This profile preserves properties and literal values while erasing local identifier distinctions.",
    "Erasing locals loses the difference between a + a and a + b. Repeated boilerplate can match. Inspect both reports and opaque-node counts.",
    [json_block("dryer-strict.json", {"schemaVersion": 1, "selection": {"include": ["src/**/*.ts"]}, "normalization": {"normalizationVersion": 1, "localIdentifiers": "erase", "properties": "preserve", "literals": "value"}}),
     block("Your project", "shell", "archguard dryer --root . --config dryer.json --json > default-report.json\narchguard dryer --root . --config dryer-strict.json --cache /tmp/dryer-extraction.json --json > strict-report.json")],
    ["docs/code-quality.md", "src/dryer/config.rs"], ["find-duplicates"])

add("duplicate-groups", "Read duplicate groups without assuming every pair matches", "review",
    "Review connected groups and follow their pair indices to the actual evidence.", ["groups", "groupsComplete", "allMembersMatch"],
    [("A matches B", "info"), ("B matches C", "info"), ("A ↔ C? Inspect pairs", "warn")],
    "groups describes connected retained pairs. allMembersMatch tells you whether every member pair appears in the evidence.",
    "Check complete, groupsComplete, problems, and omittedEvidence. A matching B and B matching C does not imply A matches C.",
    [block("Export a report", "shell", "archguard dryer --root . --config dryer.json --json > duplicate-report.json"),
     block("Node 24 · inspect-groups.mjs", "javascript", "import { readFileSync } from 'node:fs';\nconst report = JSON.parse(readFileSync('duplicate-report.json', 'utf8'));\nfor (const group of report.groups) {\n  console.log(group.members, group.allMembersMatch);\n  console.log(group.pairIndices.map(index => report.pairs[index]));\n}")],
    ["docs/code-quality.md"], ["find-duplicates"])

add("plan-mutations", "Inspect mutation edits before running tests", "review",
    "List exact runtime edits and source hashes without executing a repository command.", ["mutator plan", "mutation operators"],
    [("Runtime expression", "neutral"), ("One exact edit", "info"), ("Stored plan", "neutral")],
    "The plan lists edits such as > becoming >=, their UTF-8 byte locations, original text, and source hashes.",
    "Operators cover comparisons, equality, arithmetic, logic, updates, booleans, and zero/one literals. Type-only literals are excluded.",
    [block("Archguard checkout", "shell", "./target/debug/archguard mutator plan --root . --config examples/code-quality/plan.json --json > /tmp/domain-plan.json"),
     block("Example edit · TypeScript", "typescript", "// Original\nconst approved = amount > 100;\n// One mutation\nconst approved = amount >= 100;"),
     json_block("Your project · plan.json", {"schemaVersion": 1, "selection": {"include": ["src/domain/**/*.ts"]}})],
    ["docs/code-quality.md", "examples/code-quality/README.md"], ["run-mutations", "mutation-limits"])

add("run-mutations", "Find behavior your tests miss", "review",
    "Run weak and strengthened tests against isolated edits of the same domain functions.", ["mutator run", "baseline", "survivors"],
    [("Fresh baseline", "neutral"), ("One edit per copy", "info"), ("Killed / survived", "neutral")],
    "Compare weak and strong reports. Strengthened tests check shipping boundaries, tax, approvals, and empty totals. Original sources stay untouched.",
    "A complete run exits 0 even with survivors. A survivor can be equivalent behavior. Import, hook, timeout, and resource errors are separate outcomes.",
    [block("Archguard checkout · Node 24 · use a new profile directory", "shell", "node examples/code-quality/prepare-domain.ts /tmp/archguard-domain-profiles\n./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/weak.json --json > /tmp/weak.json\n./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/strong.json --json > /tmp/strong.json"),
     block("Run the previously inspected plan", "shell", "./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/strong.json --plan /tmp/domain-plan.json --json")],
    ["examples/code-quality/README.md", "docs/code-quality.md"], ["plan-mutations", "vitest-reporter", "mutation-reuse"], needs="Archguard checkout, Node 24, and Linux or macOS. No npm install needed for this authored example.")

add("vitest-reporter", "Connect an existing Vitest test command", "review",
    "Copy the SDK reporter into your project and declare the test command and all worker inputs.", ["Vitest reporter", "mutation protocol", "workspace dependencies"],
    [("Declared workspace", "neutral"), ("Vitest + reporter", "info"), ("Structured test evidence", "neutral")],
    "The reporter records executed tests and separates assertion failures from import, hook, and teardown errors. Only valid assertion evidence can kill an edit.",
    "Use a compatible already installed one-shot runner. The adapter was checked with Vitest 5.0.1. It does not install or discover tests, and watch mode is unsupported.",
    [block("Copy from the Archguard checkout into your project", "shell", "cp -R /path/to/irudd-ts/sdk /path/to/project/sdk"),
     json_block("Your project · mutator.json", {"schemaVersion": 1, "plan": {"schemaVersion": 1, "selection": {"include": ["src/**/*.ts"]}}, "execution": {"command": ["/absolute/path/to/node", "node_modules/vitest/vitest.mjs", "run", "--reporter=./sdk/mutator-vitest-reporter.ts"], "workspace": {"include": ["src/**", "tests/**", "sdk/**", "package.json", "package-lock.json", "vitest.config.ts"], "exclude": ["**/node_modules/**"], "dependencies": [{"source": "./node_modules", "destination": "node_modules"}]}, "limits": {"workers": 1, "maxMutants": 20}}}),
     block("Your project · replace the absolute Node path above", "shell", "archguard mutator run --root . --config mutator.json --json")],
    ["docs/mutator-test-protocol.md", "sdk/mutator-vitest-reporter.ts", "sdk/mutator.ts"], ["run-mutations", "mutation-limits"], needs="An installed compatible Vitest project, Node 24, Linux or macOS, and all test inputs declared in workspace. Adjust paths to your project.")

add("mutation-limits", "Bound a mutation run", "review",
    "Set a small mutation budget, worker count, deadline, and workspace ceiling.", ["execution limits", "cancellation"],
    [("Declared inputs", "neutral"), ("Bounded workers", "info"), ("Report or explicit limit", "warn")],
    "Limits bound trusted command execution. Reaching an analysis or execution limit remains visible in completeness and problems.",
    "Worker copies and limits are not an arbitrary-code sandbox. Address-space limits are Linux-only. SIGINT and SIGTERM request bounded cleanup.",
    [json_block("mutator.json · merge into execution.limits", {"workers": 1, "maxMutants": 20, "commandTimeoutMs": 10000, "runTimeoutMs": 120000, "maxWorkspaceFiles": 10000, "maxWorkspaceFileBytes": 10485760, "maxWorkspaceBytes": 268435456, "maxTotalWorkspaceBytes": 536870912, "maxCpuSeconds": 10, "maxGeneratedFileBytes": 10485760}),
     block("Run after applying limits", "shell", "archguard mutator run --root . --config mutator.json --json")],
    ["docs/code-quality.md", "src/mutator/config.rs"], ["vitest-reporter", "mutation-reuse"], needs="Start with a complete mutation configuration from the run or Vitest recipe. This is a limits fragment.")

add("mutation-reuse", "Reuse complete mutation results", "review",
    "Opt into conservative result reuse when every relevant test input is declared and deterministic.", ["declaredInputs", "state", "externalInputs"],
    [("Hash declared inputs", "neutral"), ("Always fresh baseline", "info"), ("Reuse complete results", "ok")],
    "Unchanged declared inputs can reuse complete killed and survived results. Changing tests, helpers, dependencies, or other declared inputs invalidates reuse.",
    "Keep state outside source and dependency inputs. Leave reuse off for network, clock, or undeclared host dependencies. Cleanup uncertainty prevents reuse.",
    [json_block("mutator.json · add execution.state", {"directory": "/tmp/project-mutation-state", "reuse": "declaredInputs", "externalInputs": ["/absolute/path/to/test-data.json"]}),
     block("Run twice with the same complete configuration", "shell", "archguard mutator run --root . --config mutator.json --json\narchguard mutator run --root . --config mutator.json --json")],
    ["docs/code-quality.md", "src/mutator/state.rs"], ["run-mutations", "mutation-limits"], needs="A complete mutation configuration. Replace externalInputs with actual required files, or use an empty list.")

add("agent-evaluation", "Have your agent try Archguard on a repository", "review",
    "Install the evaluation skill and give your agent a time, disk, and change budget.", ["irudd-ts-evaluate", "agent workflow"],
    [("Your repo + budget", "neutral"), ("Agent evaluates checks", "info"), ("Draft PR + evidence", "neutral")],
    "The agent proposes architecture checks and duplicate improvements, publishes an authorized draft PR, and adds mutation evidence when available.",
    "The skill supplies instructions, not the CLI or dependencies. Per-test attribution uses experimental research tooling when suitable; the product reporter gives aggregate evidence.",
    [block("From your Archguard checkout · target directory must be unused", "shell", "mkdir -p /path/to/project/.agents/skills\ncp -R skills/irudd-ts-evaluate /path/to/project/.agents/skills/"),
     block("Prompt for your agent", "text", "Use $irudd-ts-evaluate to evaluate this repository.\nYou have two hours and may use at most 5 GiB, including setup.\nStop before free space falls below 12%.\nYou may create and update a draft PR here. Leave it open.\nLook for useful architecture rules, duplicate reductions, and missing behavior tests.")],
    ["docs/guides/evaluate.md", "skills/irudd-ts-evaluate/SKILL.md"], ["find-duplicates", "run-mutations"], needs="An agent that can load the complete skill folder and explicitly authorized repository changes.")

add("adoption-profiles", "Try the T3 Code adoption profiles", "start",
    "Start with a concrete architecture profile from a real application.", ["adoption profiles"],
    [("Existing conventions", "neutral"), ("Candidate profile", "info"), ("Findings to review", "neutral")],
    "The profile checks selected T3 Code architecture boundaries. Review its paths and documented scope before adapting it to your project.",
    "The external checkout must match the profile. A clean selected profile does not establish whole-repository coverage or historical benchmark performance.",
    [block("Archguard checkout · supply an existing T3 Code checkout", "shell", "./target/debug/archguard check --root /absolute/path/to/t3code --config examples/t3code/profiles/t3code.json --json")],
    ["examples/t3code/README.md", "examples/t3code/rules.md", "examples/t3code/profiles/t3code.json"], ["first-check", "agent-evaluation"])

policy("rule-exceptions", "Limit a rule and allow one explicit exception", "dependencies",
       "Block filesystem imports in application source while keeping one legacy adapter out of scope.",
       dict(id="no-fs", kind="forbiddenImport", files=["src/**"], exceptions=["src/legacy.ts"], specifiers=["node:fs"]),
       {"src/main.ts": "import 'node:fs';", "src/legacy.ts": "import 'node:fs';"},
       {"src/main.ts": "export {};", "src/legacy.ts": "import 'node:fs';"},
       {"src/main.ts": "export {};", "src/legacy.ts": "import 'node:fs';", "tools/task.ts": "import 'node:fs';"},
       [("Selected source", "info"), ("Apply rule scope", "neutral"), ("Explicit exception", "neutral")],
       "Rule exceptions filter policy scope; top-level exclude changes discovery. requiredFile always checks every configured files pattern.",
       include=["src/**/*.ts", "tools/**/*.ts"], related=["block-imports", "select-sources"])

add("test-adapter", "Write a small test-command adapter", "review",
    "Use the Node-builtin SDK to report an assertion you actually executed.", ["readMutationRequest", "writeMutationResult", "custom test command"],
    [("Mutation request", "info"), ("Executed assertion", "neutral"), ("Validated result", "neutral")],
    "The fresh baseline passes. The amount >= 100 mutation fails the exact boundary assertion and receives a killed result.",
    "Only AssertionError becomes assertion evidence. Other errors remain runtime failures. A nonzero process status by itself cannot kill a mutation.",
    [block("Copy the complete SDK into your project", "shell", "cp -R /path/to/irudd-ts/sdk /path/to/project/sdk"),
     block("src/approval.ts", "typescript", "export const approved = (amount: number) => amount > 100;"),
     block("run-tests.ts", "typescript", "import { AssertionError, strictEqual } from 'node:assert';\nimport { approved } from './src/approval.ts';\nimport { readMutationRequest, writeMutationResult } from './sdk/mutator.ts';\nimport type { TestFailure } from './sdk/mutator.ts';\n\nconst request = readMutationRequest();\nconst failures: TestFailure[] = [];\ntry {\n  strictEqual(approved(100), false);\n} catch (error) {\n  failures.push({\n    kind: error instanceof AssertionError ? 'assertion' : 'runtime',\n    testId: 'approval/threshold', file: 'src/approval.ts',\n    message: String(error)\n  });\n}\nconst exitCode = failures.length ? 1 : 0;\nwriteMutationResult(request, {\n  schemaVersion: 1, requestId: request.requestId,\n  runId: request.runId, inputDigest: request.inputDigest,\n  complete: true, exitCode, reason: 'finished',\n  tests: { passed: failures.length ? 0 : 1, failed: failures.length, skipped: 0 },\n  failures\n});\nprocess.exitCode = exitCode;"),
     json_block("mutator.json", {"schemaVersion": 1, "plan": {"schemaVersion": 1, "selection": {"include": ["src/**/*.ts"]}, "operators": ["comparison"]}, "execution": {"command": ["/absolute/path/to/node", "run-tests.ts"], "workspace": {"include": ["src/**", "run-tests.ts", "sdk/**"]}, "limits": {"workers": 1, "maxMutants": 10}}}),
     block("In your project · replace the absolute Node path above", "shell", "archguard mutator run --root . --config mutator.json --json")],
    ["docs/mutator-test-protocol.md", "sdk/mutator.ts"], ["run-mutations", "vitest-reporter"], needs="Node 24, copied SDK, Linux or macOS, and an absolute path to your installed Node executable.")


# These two introductions share the same displayed-file validation as policies.
for slug, control in [
    ("first-check", {"src/client/main.ts": "import '../shared/data';", "src/shared/data.ts": "export const data = 1;", "src/server/db.ts": "export const db = {};"}),
    ("rust-sources", {"src/lib.rs": "pub mod client;\npub mod server;\npub mod shared;", "src/client.rs": "use crate::shared;\npub fn run() {}", "src/server.rs": "pub fn serve() {}", "src/shared.rs": "pub fn message() {}"}),
]:
    recipe = next(r for r in RECIPES if r["slug"] == slug)
    before, fixed = {}, {}
    for item in recipe["blocks"]:
        if item["label"].startswith("src/"):
            path = item["label"].split(" · ")[0]
            if "correction" in item["label"]:
                fixed[path] = item["code"]
            else:
                before[path] = item["code"]
    recipe["fixture"] = dict(config=json.loads(next(b["code"] for b in recipe["blocks"] if b["language"] == "json")), before=before, fixed={**before, **fixed}, control=control, rule="client-server")

for recipe in RECIPES:
    captions = {
        "public-entry": "A consumer can import @app/api. An import directly into private source is blocked.",
        "read-results": "Alternative outcomes: 0 is clean, 1 has policy violations, 2 is incomplete or invalid.",
        "duplicate-groups": "A matches B and B matches C. Inspect the separate A-to-C evidence before combining all three.",
        "classify-files": "Service and contract paths receive their roles. An unclassified file produces a finding.",
        "unique-service-id": "Orders and Payments use the same literal service identifier, producing duplicate-ID findings.",
    }
    if recipe["slug"] in captions:
        recipe["caption"] = captions[recipe["slug"]]
