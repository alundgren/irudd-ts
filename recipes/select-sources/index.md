# Choose the files Archguard analyzes

Include application sources and their local dependencies. Leave generated files out.

Category: Start here
Capabilities: include, exclude, packageManifests

You need: Built Archguard CLI on your PATH.

Diagram: Selected sources → Resolved imports → Policy checks

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "apps/**/*.ts",
    "packages/**/*.ts"
  ],
  "exclude": [
    "**/generated/**"
  ],
  "packageManifests": [
    "package.json",
    "apps/*/package.json",
    "packages/*/package.json"
  ],
  "rules": []
}
```

## Run from your project root

```shell
archguard facts --root . --config archguard.json > project-facts.json
```

## What to expect

App and shared TypeScript sources enter the graph. Generated sources do not.

## Keep in mind

An import into excluded source makes analysis incomplete. Rule and role selectors never add files to discovery.

## Reference and source

- [docs/guides/configuration.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/configuration.md)

## Related recipes

- [resolve-imports](https://alundgren.github.io/irudd-ts/recipes/resolve-imports/index.md)
- [export-graph](https://alundgren.github.io/irudd-ts/recipes/export-graph/index.md)
