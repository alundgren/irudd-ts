# Have your agent try Archguard on a repository

Install the evaluation skill and give your agent a time, disk, and change budget.

Category: Review code and tests
Capabilities: irudd-ts-evaluate, agent workflow

You need: An agent that can load the complete skill folder and explicitly authorized repository changes.

Diagram: Your repo + budget → Agent evaluates checks → Draft PR + evidence

## From your Archguard checkout · target directory must be unused

```shell
mkdir -p /path/to/project/.agents/skills
cp -R skills/irudd-ts-evaluate /path/to/project/.agents/skills/
```

## Prompt for your agent

```text
Use $irudd-ts-evaluate to evaluate this repository.
You have two hours and may use at most 5 GiB, including setup.
Stop before free space falls below 12%.
You may create and update a draft PR here. Leave it open.
Look for useful architecture rules, duplicate reductions, and missing behavior tests.
```

## What to expect

The agent proposes architecture checks and duplicate improvements, publishes an authorized draft PR, and adds mutation evidence when available.

## Keep in mind

The skill supplies instructions, not the CLI or dependencies. Per-test attribution uses experimental research tooling when suitable; the product reporter gives aggregate evidence.

## Reference and source

- [docs/guides/evaluate.md](https://github.com/alundgren/irudd-ts/blob/main/docs/guides/evaluate.md)
- [skills/irudd-ts-evaluate/SKILL.md](https://github.com/alundgren/irudd-ts/blob/main/skills/irudd-ts-evaluate/SKILL.md)

## Related recipes

- [find-duplicates](https://alundgren.github.io/irudd-ts/recipes/find-duplicates/index.md)
- [run-mutations](https://alundgren.github.io/irudd-ts/recipes/run-mutations/index.md)
