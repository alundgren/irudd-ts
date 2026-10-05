# Resolve TypeScript aliases and package imports

Use tsconfig paths, workspace package exports, and explicit source lookup settings.

Category: Start here
Capabilities: conditions, extensions, extensionAliases, requireExternalResolution

You need: Built Archguard CLI on your PATH.

Diagram: @app/domain → tsconfig / exports → Source target

## tsconfig.json

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@app/*": [
        "src/*"
      ]
    }
  }
}
```

## archguard.json

```json
{
  "schemaVersion": 1,
  "include": [
    "src/**/*.ts"
  ],
  "conditions": [
    "types",
    "import",
    "default"
  ],
  "extensions": [
    ".ts",
    ".tsx",
    ".js",
    ".json"
  ],
  "extensionAliases": [
    [
      ".js",
      [
        ".ts",
        ".tsx",
        ".js"
      ]
    ]
  ],
  "requireExternalResolution": true
}
```

## After installing your project's packages

```shell
archguard check --root . --config archguard.json --json
```

## What to expect

Archguard reads nearest and inherited tsconfigs. Required external lookups fail with exit 2 when packages are missing.

## Keep in mind

Keep requireExternalResolution false for uninstalled external packages. Source lookup is separate from compiler and runtime resolution.

## Reference and source

- [docs/guides/configuration.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/configuration.md)

## Related recipes

- [select-sources](https://alundgren.github.io/irudd-ts/recipes/select-sources/index.md)
- [public-entry](https://alundgren.github.io/irudd-ts/recipes/public-entry/index.md)
