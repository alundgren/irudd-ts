# Find behavior your tests miss

Run weak and strengthened tests against isolated edits of the same domain functions.

Category: Review code and tests
Capabilities: mutator run, baseline, survivors

You need: Archguard checkout, Node 24, and Linux or macOS. No npm install needed for this authored example.

Diagram: Fresh baseline → One edit per copy → Killed / survived

## Archguard checkout · Node 24 · use a new profile directory

```shell
node examples/code-quality/prepare-domain.ts /tmp/archguard-domain-profiles
./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/weak.json --json > /tmp/weak.json
./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/strong.json --json > /tmp/strong.json
```

## Run the previously inspected plan

```shell
./target/debug/archguard mutator run --root . --config /tmp/archguard-domain-profiles/strong.json --plan /tmp/domain-plan.json --json
```

## What to expect

Compare weak and strong reports. Strengthened tests check shipping boundaries, tax, approvals, and empty totals. Original sources stay untouched.

## Keep in mind

A complete run exits 0 even with survivors. A survivor can be equivalent behavior. Import, hook, timeout, and resource errors are separate outcomes.

## Reference and source

- [examples/code-quality/README.md](https://github.com/alundgren/irudd-ts/blob/main/examples/code-quality/README.md)
- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)

## Related recipes

- [plan-mutations](https://alundgren.github.io/irudd-ts/recipes/plan-mutations/index.md)
- [vitest-reporter](https://alundgren.github.io/irudd-ts/recipes/vitest-reporter/index.md)
- [mutation-reuse](https://alundgren.github.io/irudd-ts/recipes/mutation-reuse/index.md)
