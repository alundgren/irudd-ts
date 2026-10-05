# Export compiler facts by context

Inspect inferred receiver types and public member status separately from source graph facts.

Category: Add your own checks
Capabilities: semantic-facts, SemanticFacts, SemanticRule

You need: Complete the inferred-member recipe first. Run the SDK snippet from the Archguard checkout.

Diagram: Explicit tsconfig → SemanticFacts v1 → Member status

## Archguard checkout · after the provider setup

```shell
./target/debug/archguard semantic-facts --root examples/semantic --config examples/semantic/archguard.json --semantic-config examples/semantic/semantic.json > /tmp/semantic-facts.json
```

## inspect-semantic.ts · read host-validated facts from stdin

```typescript
import { readSemanticFacts, missingMemberDiagnostics } from './sdk/index.ts';

const facts = readSemanticFacts();
console.log(missingMemberDiagnostics(facts, 'missing-member'));
```

## Archguard checkout · Node 24

```shell
node inspect-semantic.ts < /tmp/semantic-facts.json
```

## What to expect

The export includes context-specific dot-access facts, member states, compiler diagnostics, and completion problems.

## Keep in mind

Computed and private properties are outside this contract. Public optional access and JSX member tags are included. Context selectors never add sources.

## Reference and source

- [docs/extensions/semantic-provider.md](https://github.com/alundgren/irudd-ts/blob/main/docs/extensions/semantic-provider.md)
- [sdk/semantic.ts](https://github.com/alundgren/irudd-ts/blob/main/sdk/semantic.ts)

## Related recipes

- [missing-member](https://alundgren.github.io/irudd-ts/recipes/missing-member/index.md)
- [export-graph](https://alundgren.github.io/irudd-ts/recipes/export-graph/index.md)
