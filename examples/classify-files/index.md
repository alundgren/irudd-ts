# Give every file a known role

Catch files that land outside your folder conventions.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "repository": {
    "roles": [
      {
        "id": "service",
        "files": ["src/services/**"]
      },
      {
        "id": "contract",
        "files": ["src/contracts/**"]
      },
      {
        "id": "test",
        "files": ["src/*.test.ts"]
      }
    ],
    "rules": [
      {
        "id": "classify-source",
        "kind": "classified",
        "files": ["src/**"]
      }
    ]
  }
}
```

src/misc.ts:

```typescript
export {};
```

Archguard reports:

```text
src/misc.ts:1:1: classify-source: expected exactly one repository role, found 0
```

Fix · src/contracts/message.ts:

```typescript
export {};
```

Fix:

```shell
rm src/misc.ts
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md
