# Review similar functions

Group selected functions by structural similarity.

dryer.json:

```json
{
  "schemaVersion": 1,
  "selection": {
    "include": ["src/**/*.ts", "src/**/*.tsx"]
  }
}
```

Run:

```shell
archguard dryer --root . --config dryer.json
```

The output lists pairs and groups with source locations. Use the reported pairs to review matches within a group, and compare behavior before sharing an implementation.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md
