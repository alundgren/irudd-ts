# Keep a package free of a dependency

Stop a domain workspace package from declaring React.

Category: Control dependencies
Capabilities: packageDependency

You need: Built Archguard CLI on your PATH.

Diagram: @app/domain → react dependency · blocked

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "rules": [
    {
      "id": "domain-no-react",
      "kind": "packageDependency",
      "files": [
        "packages/domain/package.json"
      ],
      "specifiers": [
        "react",
        "react-dom"
      ]
    }
  ]
}
```

## packages/domain/package.json · before

```json
{"name":"@app/domain","dependencies":{"react":"*"}}
```

## src/main.ts · before

```typescript
export {};
```

## packages/domain/package.json · correction

```json
{"name":"@app/domain","dependencies":{}}
```

## Run from this project directory

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Before: exit 1, finding domain-no-react. After the correction: exit 0.

## Keep in mind

This checks production, peer, and optional dependencies. Development dependencies are outside the current package facts.

## Reference and source

- [docs/guides/rules.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md)

## Related recipes

- [public-entry](https://alundgren.github.io/irudd-ts/recipes/public-entry/index.md)
