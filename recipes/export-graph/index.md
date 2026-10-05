# Export the source dependency graph

Feed versioned project facts into your own reports or graph tools.

Category: Add your own checks
Capabilities: facts, ProjectFacts

You need: Built Archguard CLI on your PATH.

Diagram: Selected source → ProjectFacts v1 → Your report

## In your project

```shell
archguard facts --root . --config archguard.json > project-facts.json
```

## Node 24 · inspect-graph.mjs

```javascript
import { readFileSync } from 'node:fs';
const project = JSON.parse(readFileSync('project-facts.json', 'utf8'));
for (const file of project.files) {
  console.log(file.path, file.imports);
}
```

## What to expect

project-facts.json contains sources, import resolution states, exports, recognized calls and services, and package facts.

## Keep in mind

Keep every import's resolution status and analysis problems. A selected dependency closure does not establish whole-repository coverage.

## Reference and source

- [sdk/README.md](https://github.com/alundgren/irudd-ts/blob/main/sdk/README.md)
- [src/facts.rs](https://github.com/alundgren/irudd-ts/blob/main/src/facts.rs)

## Related recipes

- [typescript-plugin](https://alundgren.github.io/irudd-ts/recipes/typescript-plugin/index.md)
- [compiler-facts](https://alundgren.github.io/irudd-ts/recipes/compiler-facts/index.md)
