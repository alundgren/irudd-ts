"""Maintained examples. Policy configs and files also drive real CLI checks in check.py."""
import json
import re

RECIPES = []


def block(label, language, code):
    return {"label": label, "language": language, "code": code.strip()}


def json_block(label, value):
    text = json.dumps(value, indent=2)
    # Keep short lists of strings on one line so configs stay readable.
    text = re.sub(r'\[\n\s*("[^"\n]*"(?:,\n\s*"[^"\n]*")*)\n\s*\]',
                  lambda m: "[" + re.sub(r',\n\s*', ', ', m.group(1)) + "]" if len(m.group(1)) < 70 else m.group(0), text)
    return block(label, "json", text)


def add(slug, title, summary, capabilities, nodes, blocks, result, reference, fixture=None):
    RECIPES.append(dict(slug=slug, title=title, summary=summary, capabilities=capabilities,
                        nodes=nodes, blocks=blocks, result=result, reference=reference,
                        fixture=fixture))


def policy(slug, title, summary, rule, before, fixed, control, nodes, report, *,
           include=None, extra=None, repository=False, show=None):
    """A miniature project: before fails with exit 1, fixed and control pass with exit 0."""
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
    for path in show or changed:
        blocks.append(block(path, "json" if path.endswith(".json") else "typescript", before[path]))
    blocks.append(block("Archguard reports", "output", report))
    for path in changed:
        blocks.append(block("Fix · " + path, "json" if path.endswith(".json") else "typescript", fixed[path]))
    for path in removed:
        blocks.append(block("Fix", "shell", "rm " + path))
    ref = "docs/guides/repository-rules.md" if repository else "docs/guides/rules.md"
    add(slug, title, summary, ["repository." + rule["kind"] if repository else rule["kind"]], nodes, blocks,
        "Exit 1 with the finding above. After the fix, exit 0.", ref,
        fixture=dict(config=config, before=before, fixed=fixed, control=control, rule=rule["id"], report=report))


policy("block-server", "Keep server code out of the client",
       "Each import looks fine on its own. Archguard follows the whole path and flags the client reaching the server.",
       dict(id="client-server", kind="forbiddenDependency", files=["src/client/**"], targets=["src/server/**"], transitive=True),
       {"src/client/main.ts": "import '../shared/data';", "src/shared/data.ts": "import '../server/db';", "src/server/db.ts": "export const db = {};"},
       {"src/client/main.ts": "export {};", "src/shared/data.ts": "import '../server/db';", "src/server/db.ts": "export const db = {};"},
       {"src/client/main.ts": "import '../shared/data';", "src/shared/data.ts": "export const data = 1;", "src/server/db.ts": "import '../shared/data';"},
       [("client/main.ts", "info"), ("shared/data.ts", "neutral"), ("server/db.ts · blocked", "danger")],
       "src/client/main.ts:1:1: client-server: forbidden dependency on src/server/db.ts\n  src/client/main.ts -> src/shared/data.ts -> src/server/db.ts",
       show=["src/client/main.ts", "src/shared/data.ts"])

policy("public-entry", "Make other code use a package's public entry",
       "Stop code from reaching into a workspace package's private files.",
       dict(id="api-public-entry", kind="publicEntry", files=["src/**"], targets=["packages/api/src/**"], specifiers=["@app/api"]),
       {"packages/api/package.json": '{"name":"@app/api","exports":{".":"./src/index.ts"}}', "packages/api/src/index.ts": "export const api = 1;", "src/main.ts": "import '../packages/api/src/index';"},
       {"packages/api/package.json": '{"name":"@app/api","exports":{".":"./src/index.ts"}}', "packages/api/src/index.ts": "export const api = 1;", "src/main.ts": "import '@app/api';"},
       {"src/main.ts": "import './helper';", "src/helper.ts": "export {};"},
       [("src/main.ts", "info"), ("@app/api · allowed", "ok"), ("Private path · blocked", "danger")],
       "src/main.ts:1:1: api-public-entry: import packages/api/src/index.ts through its declared public package entry",
       include=["src/**/*.ts", "packages/**/*.ts"])

policy("package-dependencies", "Keep a package's dependency list clean",
       "Stop a domain package from picking up React.",
       dict(id="domain-no-react", kind="packageDependency", files=["packages/domain/package.json"], specifiers=["react", "react-dom"]),
       {"packages/domain/package.json": '{"name":"@app/domain","dependencies":{"react":"*"}}', "src/main.ts": "export {};"},
       {"packages/domain/package.json": '{"name":"@app/domain","dependencies":{}}', "src/main.ts": "export {};"},
       {"packages/domain/package.json": '{"name":"@app/domain","devDependencies":{"react":"*"}}', "src/main.ts": "export {};"},
       [("@app/domain", "info"), ("react · blocked", "danger")],
       "packages/domain/package.json:1:1: domain-no-react: package @app/domain must not depend on react")

policy("block-calls", "Keep runtime calls at the edge",
       "Flag domain code that imports a module calling Effect.runPromise, directly or through a helper.",
       dict(id="domain-no-run", kind="forbiddenCall", files=["src/domain/**"], origins=["effect/Effect#runPromise"], transitive=True),
       {"src/domain/orders.ts": "import '../runner';", "src/runner.ts": "import * as E from 'effect/Effect';\nE.runPromise(task);"},
       {"src/domain/orders.ts": "export const order = 1;", "src/runner.ts": "import * as E from 'effect/Effect';\nE.runPromise(task);"},
       {"src/domain/orders.ts": "import * as E from 'effect/Effect';\nfunction run(E: any) { E.runPromise(task); }"},
       [("domain/orders.ts", "info"), ("runner.ts", "neutral"), ("runPromise · blocked", "danger")],
       "src/domain/orders.ts:1:1: domain-no-run: reachable dependency src/runner.ts calls effect/Effect#runPromise\n  src/domain/orders.ts -> src/runner.ts -> src/runner.ts:36",
       show=["src/domain/orders.ts", "src/runner.ts"])

service = "import * as C from 'effect/Context';\nexport class Orders extends C.Service<Orders, {}>()('app/Orders') {}"

policy("service-namespace", "Enforce a team import convention",
       "T3 Code imports Effect services as namespaces. Three pull requests fixed this by hand.",
       dict(id="service-namespace", kind="serviceNamespace", files=["src/**"]),
       {"src/orders.ts": service + "\nexport const layer = 1;", "src/main.ts": "import { Orders, layer } from './orders';"},
       {"src/orders.ts": service + "\nexport const layer = 1;", "src/main.ts": "import * as Orders from './orders';"},
       {"src/orders.ts": service, "src/main.ts": "import type { Orders } from './orders';"},
       [("src/main.ts", "info"), ("import * as Orders", "ok"), ("Service module", "neutral")],
       "src/main.ts:1:1: service-namespace: import service module src/orders.ts as a namespace instead of Orders\n  src/orders.ts\nsrc/main.ts:1:1: service-namespace: import service module src/orders.ts as a namespace instead of layer\n  src/orders.ts")

policy("classify-files", "Give every file a known role",
       "Catch files that land outside your folder conventions.",
       dict(id="classify-source", kind="classified", files=["src/**"]),
       {"src/services/orders.ts": "export {};", "src/misc.ts": "export {};"},
       {"src/services/orders.ts": "export {};", "src/contracts/message.ts": "export {};"},
       {"src/services/orders.ts": "export {};", "src/contracts/message.ts": "export {};", "src/main.test.ts": "export {};"},
       [("services/", "info"), ("contracts/", "info"), ("src/misc.ts · no role", "danger")],
       "src/misc.ts:1:1: classify-source: expected exactly one repository role, found 0",
       extra={"repository": {"roles": [{"id": "service", "files": ["src/services/**"]}, {"id": "contract", "files": ["src/contracts/**"]}, {"id": "test", "files": ["src/*.test.ts"]}]}},
       repository=True, show=["src/misc.ts"])

policy("companions", "Require files that belong together",
       "Every service needs its layer file next to it.",
       dict(id="service-companion", kind="companion", role="service", replace=["/services/", "/layers/"]),
       {"src/services/Orders.ts": "export class Orders {}"},
       {"src/services/Orders.ts": "export class Orders {}", "src/layers/Orders.ts": "export const live = 1;"},
       {"src/services/Orders.test.ts": "export {};"},
       [("services/Orders.ts", "info"), ("layers/Orders.ts · missing", "danger")],
       "src/services/Orders.ts:1:1: service-companion: required analyzed companion is absent: src/layers/Orders.ts\n  src/services/Orders.ts -> src/layers/Orders.ts",
       extra={"repository": {"roles": [{"id": "service", "files": ["src/services/*.ts"], "exclude": ["**/*.test.ts"]}]}},
       repository=True, show=["src/services/Orders.ts"])

policy("registry-imports", "Never forget to register a migration",
       "The registry must import every migration file.",
       dict(id="migration-import", kind="registryImport", role="migration", registry="src/migrations.ts"),
       {"src/migrations/001.ts": "export default 1;", "src/migrations.ts": "export const migrations = [];"},
       {"src/migrations/001.ts": "export default 1;", "src/migrations.ts": "import first from './migrations/001';\nexport const migrations = [first];"},
       {"src/migrations/001.test.ts": "export {};", "src/migrations.ts": "export const migrations = [];"},
       [("migrations/001.ts", "info"), ("migrations.ts · no import", "danger")],
       "src/migrations/001.ts:1:1: migration-import: role migration requires a direct static value import in src/migrations.ts\n  src/migrations.ts -> src/migrations/001.ts",
       extra={"repository": {"roles": [{"id": "migration", "files": ["src/migrations/*.ts"], "exclude": ["**/*.test.ts"]}]}},
       repository=True)

add("typescript-plugin", "Write your own rule over the whole graph",
    "Plugins get the same resolved graph as the built-in rules. This one does the client-to-server check by hand.",
    ["plugins", "TypeScript SDK", "Rust SDK", "facts"],
    [("Resolved graph", "info"), ("Your rule", "neutral"), ("Findings", "neutral")],
    [json_block("archguard.json", {"schemaVersion": 1, "include": ["src/**/*.ts"], "plugins": [{"name": "team-rules", "command": ["node", "./team-rules.ts"]}]}),
     block("team-rules.ts · Node 24, with sdk/ copied from the Archguard repository", "typescript", "import { createDependencyQuery, runPlugin } from './sdk/index.ts';\n\nrunPlugin(project => {\n  const dependencies = createDependencyQuery(project);\n  return project.files.flatMap(file => {\n    if (!file.path.startsWith('src/client/')) return [];\n    return [...dependencies(file.path)]\n      .filter(([target]) => target.startsWith('src/server/'))\n      .map(([target, path]) => ({\n        rule: 'client-server', file: file.path, offset: 0,\n        message: `Client reaches ${target}`, evidence: path\n      }));\n  });\n});")],
    "Plugin findings show up next to built-in findings, with the same exit codes. Rust rules use the same graph. archguard facts exports it as JSON.",
    "docs/extensions/plugins.md")

add("find-duplicates", "Find copy-pasted logic",
    "Groups functions with the same structure, even after variables were renamed.",
    ["dryer"],
    [("Your functions", "neutral"), ("Compare structure", "info"), ("Similar groups", "neutral")],
    [json_block("dryer.json", {"schemaVersion": 1, "selection": {"include": ["src/**/*.ts", "src/**/*.tsx"]}}),
     block("Run", "shell", "archguard dryer --root . --config dryer.json")],
    "You get pairs and groups of similar functions with their locations. Not every pair in a group matches. You decide what to merge.",
    "docs/code-quality.md")

add("run-mutations", "Find code changes your tests miss",
    "Archguard makes small edits, like >= to >, and runs your tests on a copy. An edit no test notices shows a gap.",
    ["mutator"],
    [("subtotal >= 100", "neutral"), ("subtotal > 100", "info"), ("Tests still pass · gap", "warn")],
    [block("Run · mutator.json names the source files and your test command", "shell", "archguard mutator run --root . --config mutator.json"),
     block("Trimmed output · the bundled example's weak tests", "output", "Survived examples/code-quality/domain.ts:9 bytes 265..267  >= -> >\n  original:   return purchase.subtotal >= 100 && purchase.domestic;\n  mutant:     return purchase.subtotal > 100 && purchase.domestic;")],
    "Each surviving edit names the line and the change. Some edits change nothing real, so read each one before adding a test. Archguard never edits your files. It works on copies.",
    "docs/code-quality.md")

add("agent-evaluation", "Let your agent try Archguard on your repo",
    "Give your agent a time and disk budget. It looks for rules that fit, copy-pasted code, and test gaps.",
    ["irudd-ts-evaluate"],
    [("Your repo + budget", "neutral"), ("Agent tries Archguard", "info"), ("Findings · draft PR if allowed", "neutral")],
    [block("Prompt for your agent", "text", "Install the irudd-ts-evaluate skill from github.com/alundgren/irudd-ts\n(skills/irudd-ts-evaluate) in your personal skills folder, not in this\nrepository. Use it to evaluate this repository.\nYou have eight hours and may use at most 20 GiB of disk, including setup.\nStop before free space falls below 12%.\nYou may create a draft PR here. Leave it open.")],
    "The agent proposes rules backed by your docs or past fixes, and reports anything it could not check.",
    "docs/guides/evaluate.md")
