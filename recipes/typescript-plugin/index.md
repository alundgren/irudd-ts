# Add a TypeScript graph plugin

Run a custom rule over the same graph as built-in policies.

Category: Add your own checks
Capabilities: plugins, TypeScript SDK, createDependencyQuery

You need: Archguard CLI, Node 24, and the complete sdk/ directory for the custom example. Unix for subprocess plugins.

Diagram: ProjectFacts v1 → Explicit Node command → Custom findings

## Archguard checkout · Node 24

```shell
cargo build --locked --bins --examples
./target/debug/archguard check --root examples/graph-plugin --config examples/graph-plugin/archguard.json --json
```

## examples/graph-plugin/shared/message.ts · try a violation

```typescript
import '../server/db.ts';
export const message = 'hello';
```

## Minimal plugin registration · archguard.json

```json
{
  "schemaVersion": 1,
  "plugins": [
    {
      "name": "team-rules",
      "command": [
        "node",
        "./team-rules.ts"
      ],
      "timeoutMs": 5000
    }
  ]
}
```

## team-rules.ts · SDK copied to ./sdk

```typescript
import { createDependencyQuery, runPlugin } from './sdk/index.ts';

runPlugin(project => {
  const dependencies = createDependencyQuery(project);
  return project.files.flatMap(file => {
    if (!file.path.startsWith('src/client/')) return [];
    return [...dependencies(file.path)]
      .filter(([target]) => target.startsWith('src/server/'))
      .map(([target, path]) => ({
        rule: 'client-server', file: file.path, offset: 0,
        message: `Client reaches ${target}`, evidence: path
      }));
  });
});
```

## What to expect

The bundled project is clean: exit 0 with no findings. Apply the shared/message.ts edit below and rerun to report client → shared → server, exit 1. Restore the file afterward.

## Keep in mind

Plugins are trusted commands, run from the configuration directory. Reuse one dependency query for many roots. Process limits are not a sandbox.

## Reference and source

- [docs/extensions/plugins.md](https://github.com/alundgren/irudd-ts/blob/main/docs/extensions/plugins.md)
- [examples/graph-plugin/plugin.ts](https://github.com/alundgren/irudd-ts/blob/main/examples/graph-plugin/plugin.ts)
- [sdk/index.ts](https://github.com/alundgren/irudd-ts/blob/main/sdk/index.ts)

## Related recipes

- [rust-plugin](https://alundgren.github.io/irudd-ts/recipes/rust-plugin/index.md)
- [export-graph](https://alundgren.github.io/irudd-ts/recipes/export-graph/index.md)
