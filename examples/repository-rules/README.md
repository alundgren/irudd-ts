# Repository conventions

The clean source project has numbered migrations, a registry, service/layer companions, tool contracts/handlers/tests, shared contracts, and an API host. [archguard.json](archguard.json) assigns roles and checks their relationships.

```sh
cargo build --locked --bins --examples
./target/debug/archguard check --root examples/repository-rules --config examples/repository-rules/archguard.json
./target/debug/archguard facts --root examples/repository-rules --config examples/repository-rules/archguard.json > /tmp/archguard-role-facts.json
./target/debug/examples/role_inventory examples/repository-rules/archguard.json < /tmp/archguard-role-facts.json
```

Add `src/services/Missing.ts` without its layer to see a companion finding, then add `src/layers/Missing.ts` to correct it. A migration module must have a static value import in `src/migrations.ts`; this prerequisite does not prove membership in the exported registry array.

[Repository rules](../../docs/guides/repository-rules.md) explains the selectors and evidence limits. Product tests exercise failure, correction, and negative controls against this example policy.
