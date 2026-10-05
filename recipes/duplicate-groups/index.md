# Read duplicate groups without assuming every pair matches

Review connected groups and follow their pair indices to the actual evidence.

Category: Review code and tests
Capabilities: groups, groupsComplete, allMembersMatch

You need: Built Archguard CLI on your PATH.

Diagram: A matches B and B matches C. Inspect the separate A-to-C evidence before combining all three.

## Export a report

```shell
archguard dryer --root . --config dryer.json --json > duplicate-report.json
```

## Node 24 · inspect-groups.mjs

```javascript
import { readFileSync } from 'node:fs';
const report = JSON.parse(readFileSync('duplicate-report.json', 'utf8'));
for (const group of report.groups) {
  console.log(group.members, group.allMembersMatch);
  console.log(group.pairIndices.map(index => report.pairs[index]));
}
```

## What to expect

groups describes connected retained pairs. allMembersMatch tells you whether every member pair appears in the evidence.

## Keep in mind

Check complete, groupsComplete, problems, and omittedEvidence. A matching B and B matching C does not imply A matches C.

## Reference and source

- [docs/code-quality.md](https://github.com/alundgren/irudd-ts/blob/main/docs/code-quality.md)

## Related recipes

- [find-duplicates](https://alundgren.github.io/irudd-ts/recipes/find-duplicates/index.md)
