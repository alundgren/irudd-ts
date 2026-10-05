# Restrict a package dependency

Report React dependencies in a selected domain package's manifest.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "rules": [
    {
      "id": "domain-no-react",
      "kind": "packageDependency",
      "files": ["packages/domain/package.json"],
      "specifiers": ["react", "react-dom"]
    }
  ]
}
```

packages/domain/package.json:

```json
{"name":"@app/domain","dependencies":{"react":"*"}}
```

Archguard reports:

```text
packages/domain/package.json:1:1: domain-no-react: package @app/domain must not depend on react
```

Fix · packages/domain/package.json:

```json
{"name":"@app/domain","dependencies":{}}
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/rules.md
