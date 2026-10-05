# Evaluate checks with an agent

Review candidate policies, structural matches, and mutation results within a time and disk budget.

Prompt for your agent:

```text
Install the irudd-ts-evaluate skill from github.com/alundgren/irudd-ts
(skills/irudd-ts-evaluate) in your personal skills folder, not in this
repository. Use it to evaluate this repository.
You have eight hours and may use at most 20 GiB of disk, including setup.
Stop before free space falls below 12%.
You may create a draft PR here. Leave it open.
```

The agent proposes rules backed by your docs or past fixes, and reports anything it could not check.

Reference: https://github.com/alundgren/irudd-ts/blob/main/docs/guides/evaluate.md
