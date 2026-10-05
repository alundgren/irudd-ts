# Evaluate Archguard with an agent

The [irudd-ts-evaluate skill](../../skills/irudd-ts-evaluate/SKILL.md) helps an
agent try Archguard against an existing repository. It asks for time and disk
budgets and permission to create/update a draft PR. Evaluation starts with
project guidance, historical fixes, architecture checks, and duplication review.
It publishes a useful draft before mutation execution, then updates it as
mutation results support changes. Agree on useful completion and compact
evidence retention too. The budget is a ceiling; a completed declared sample
with reviewed findings can finish sooner.

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
remain explicit. Import controls include relevant roots, subpaths, and aliases.
Duplication review can reasonably produce no refactor. Zero observed exclusive
mutant kills does not establish a safe test deletion.

The default mutation pool includes every active test across declared projects
on the chosen platform, even projects omitted from routine checks. Fit the
budget by reducing the mutant sample first. A narrower pool needs a recorded
task, measured budget, or platform reason, with conclusions limited to that
pool. Native integrations may need another platform.

Before execution, record source discovery, sampled files/sites, the test pool,
and required patch validation separately. Check whether existing scenarios
enter the selected branches with inputs and assertions that distinguish the
edits. Test names alone do not establish execution or coverage.

Prefer product `archguard mutator run` for aggregate campaigns. Complex builds
may need an adapter. Full per-test attribution uses experimental research
tooling when required and suitable, with its setup and limitations disclosed;
the product reporter supplies aggregate counts and assertion failure IDs. The
evaluation compares baseline and mutant active/skip counts with the frozen
pool, since the product classifier does not enforce that comparison. Count
mismatches remain unknown even if the product reports killed or survived.
Matching counts do not prove that the same test IDs ran; full inventory claims
need an inventory-capable adapter. The current research adapter rejects
name-filtered skipped inventory entries. Use
complete file/project-level focused pools or a verified adapter supporting an
explicit inventory, without dropping skips after outcomes are known.

Keep assertion kills, survivors, and unknown results distinct. Explain
mutant-caused runtime failures separately from setup/build/environment failures,
without claiming unavailable attribution. Reports name actual scopes, results,
costs, validation, and retained evidence. Keep shared inventories once and avoid
repeated source snapshots. Describe regression fixtures and documentation
corrections separately from production migrations or new schema versions.
Useful static findings can still be delivered when mutation is unavailable.
