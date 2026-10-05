# Write your own rule over the whole graph

Plugins get the same resolved graph as the built-in rules. This one does the client-to-server check by hand.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "plugins": [
    {
      "name": "team-rules",
      "command": ["node", "./team-rules.ts"]
    }
  ]
}
```

team-rules.ts · Node 24, with sdk/ copied from the Archguard repository:

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

Plugin findings show up next to built-in findings, with the same exit codes. Rust rules use the same graph. archguard facts exports it as JSON.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/extensions/plugins.md
