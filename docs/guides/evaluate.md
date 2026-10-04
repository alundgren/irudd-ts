# Evaluate Archguard with an agent

The [irudd-ts-evaluate skill](../../skills/irudd-ts-evaluate/SKILL.md) helps an
agent try Archguard against an existing repository. It asks for time and disk
budgets and permission to create/update a draft PR. Evaluation starts with
project guidance, historical fixes, architecture checks, and duplication review.
It publishes a useful draft before mutation execution, then updates it as
mutation results support changes.

Install the complete `skills/irudd-ts-evaluate` folder from `alundgren/irudd-ts`
with your agent's skill installer. Preserve its bundled references. For agents
that discover project skills in `.agents/skills`, copy it from your Archguard
checkout into an unused directory in the target project:

```sh
mkdir -p /path/to/project/.agents/skills
cp -R /path/to/irudd-ts/skills/irudd-ts-evaluate /path/to/project/.agents/skills/
```

Use a personal skill directory if the target repository should stay unchanged.
Installation supplies instructions, not the CLI or test dependencies. The
agent checks existing tools and counts setup within the agreed budget.

Ask the agent:

```text
Use $irudd-ts-evaluate to evaluate this repository.
You have eight hours and may add at most 20 GiB of disk use, including setup.
Stop before free space falls to 12%. Use the isolated test VM.
You may create a draft PR here and update it as results arrive. Leave it open.
Look for architecture rules, duplication reductions, redundant tests, and
missing behavior tests. Include every supported test project.
```

Architecture proposals cite project guidance or specific fixes. Unsupported
language features, incomplete analysis, unavailable runners, and budget stops
remain explicit. Zero observed exclusive mutant kills signals a review
candidate; it does not establish a safe test deletion.

All tests and all mutants are separate scopes. The full active test pool can
run against a budgeted sample of production mutation sites. Reports name source
selection, platform exclusions, sample, completed results, costs, and validation.
Native integrations may need another platform.

The skill is an agent workflow, not a universal runner. Complex builds may
need an adapter. Full per-test attribution currently uses experimental research
tooling when suitable; the product reporter supplies aggregate counts and
assertion failure IDs. Useful static findings can still be delivered when that
later phase is unavailable.
