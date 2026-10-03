# T3 Code adoption case study

These profiles illustrate architecture policies for T3 Code. They are independent configurations, not automatically combined presets or a universal recommended ruleset.

| Profile | Purpose |
| --- | --- |
| [t3code.json](profiles/t3code.json) | Default adoption candidate with 25 configured policy instances |
| [t3code-mobile-android.json](profiles/t3code-mobile-android.json), [t3code-mobile-ios.json](profiles/t3code-mobile-ios.json) | Separate mobile source lookup and cycle policies |
| [t3code-structure.json](profiles/t3code-structure.json) | File-role and companion simulation with nine roles and eleven rules |
| [t3code-legacy-boundary.json](profiles/t3code-legacy-boundary.json) | Historical migration policy for retired sources |
| [t3code-experimental.json](profiles/t3code-experimental.json) | Experimental policies requiring review before adoption |

```sh
cargo build --release --locked
./target/release/archguard check --root /path/to/t3code --config examples/t3code/profiles/t3code.json --json
```

Selectors apply to the upstream root. Review policy groups and exceptions for the revision you scan. A profile entry count is not a count of demonstrated historical bugs. The [rule catalog](rules.md) distinguishes current adoption candidates, migration policies, and experiments.

Historical reductions, copied-source notices, structural simulations, installed-source coverage, raw measurements, and archived reports live under [research/t3code](../../research/t3code/README.md). No T3 dependency installation is needed to build Archguard or run its small examples.
