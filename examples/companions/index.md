# Require files that belong together

Every service needs its layer file next to it.

archguard.json:

```json
{
  "schemaVersion": 1,
  "include": ["src/**/*.ts"],
  "repository": {
    "roles": [
      {
        "id": "service",
        "files": ["src/services/*.ts"],
        "exclude": ["**/*.test.ts"]
      }
    ],
    "rules": [
      {
        "id": "service-companion",
        "kind": "companion",
        "role": "service",
        "replace": ["/services/", "/layers/"]
      }
    ]
  }
}
```

src/services/Orders.ts:

```typescript
export class Orders {}
```

Archguard reports:

```text
src/services/Orders.ts:1:1: service-companion: required analyzed companion is absent: src/layers/Orders.ts
  src/services/Orders.ts -> src/layers/Orders.ts
```

Fix · src/layers/Orders.ts:

```typescript
export const live = 1;
```

Exit 1 with the finding above. After the fix, exit 0.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/repository-rules.md
