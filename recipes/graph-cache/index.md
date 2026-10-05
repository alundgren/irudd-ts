# Reuse a validated source graph

Keep a complete graph between checks while still rerunning policies and plugins.

Category: Add your own checks
Capabilities: cache

You need: Built Archguard CLI on your PATH.

Diagram: Revalidate inputs → Reuse complete graph → Rerun every policy

## In your project · run twice

```shell
archguard check --root . --config archguard.json --cache /tmp/project.archguard-cache.json --json
archguard check --root . --config archguard.json --cache /tmp/project.archguard-cache.json --json
```

## What to expect

The first complete check writes the cache. An unchanged second check can reuse syntax and resolved imports.

## Keep in mind

Keep the cache outside selected inputs. Compiler facts rerun independently. Recorded cache measurements were slower; reuse counts do not prove a speedup.

## Reference and source

- [docs/guides/cache.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/cache.md)

## Related recipes

- [export-graph](https://alundgren.github.io/irudd-ts/recipes/export-graph/index.md)
- [missing-member](https://alundgren.github.io/irudd-ts/recipes/missing-member/index.md)
