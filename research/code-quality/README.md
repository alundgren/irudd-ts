# Code-quality evidence for coding agents

This directory retains the initial research prototypes for structural duplicate detection and mutation testing. They produce evidence for a reviewer, with source locations, raw outcomes, input provenance, and explicit limitations. The later [product dogfooding](product/FINDINGS.md) uses Archguard's own CLI and SDK; [the adoption guide](../../docs/code-quality.md) documents those commands. Neither phase adds CI or quality gates.

Start with [the short findings](findings.md). The [interactive HTML report](report.html) lets you explore the recorded clone measures and thresholds. The [experiment protocol](protocol.md), [environment record](environment.json), and [author context](author-context.md) document the shared decisions and inputs. [Validation and review](validation.md) records the completed checks and child PRs.

| Experiment | What to run and inspect |
| --- | --- |
| [Dryer](dryer/README.md) | Pinned reference implementation, original TypeScript detector, jscpd comparison, authored A-F controls, ten-file T3 corpus, parser failures, and independent-review corrections |
| [Mutator](mutator/README.md) | Pinned reference implementation, Stryker/Vitest comparison, weak and corrected tests, selected T3 reduction, execution errors, cache invalidation, and actionable survivor JSON |

Each directory has its own locked installation, license inventory, reproduction commands, and retained raw evidence. No root npm workspace or product installation imports them. Use the complete commands in the experiment READMEs and explicit external checkout paths. Reserve a timing window across agents and run the required release build before recording new costs.

The frozen T3 revision is `31a9da179ed0763335f05681c577474aec5d2309`. The duplicate detector uses complete selected source modules. Mutation tests use a precisely labeled reduced reproduction with an adapted test import. Neither trial establishes whole-repository coverage.

A reviewer should receive the relevant implementation/specification alongside the tool evidence. The useful question is whether a candidate exposes an accidental copy or unconstrained behavior. Intentional parallel code, equivalent mutations, and unimportant implementation details can be retained without chasing a score.
