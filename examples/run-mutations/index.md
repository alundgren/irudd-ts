# Review edits that survive your tests

Run selected tests against small edits, such as changing >= to >, in isolated copies.

Run · mutator.json names the source files and your test command:

```shell
archguard mutator run --root . --config mutator.json
```

Trimmed output · the bundled example's weak tests:

```text
Survived examples/code-quality/domain.ts:9 bytes 265..267  >= -> >
  original:   return purchase.subtotal >= 100 && purchase.domestic;
  mutant:     return purchase.subtotal > 100 && purchase.domestic;
```

Each surviving edit names the line and the change. Some edits change nothing real, so read each one before adding a test. Archguard never edits your files. It works on copies.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md
