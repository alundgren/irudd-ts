# Find copy-pasted logic

Groups functions with the same structure, even after variables were renamed.

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

You get pairs and groups of similar functions with their locations. Not every pair in a group matches. You decide what to merge.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md
