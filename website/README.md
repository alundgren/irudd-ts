# Archguard site

The public site is at [alundgren.github.io/irudd-ts](https://alundgren.github.io/irudd-ts/).
It answers one question for someone with an existing repository: what does
Archguard add to the linter they already run? The home page makes that case.
A short list of examples shows each point in action. Setup, options, and
experiments stay in the [reference documentation](../docs/README.md), which
readers' agents can follow from `llms.txt`.

It is a static GitHub Pages project site. Python 3 builds HTML, Markdown copies,
`examples.json`, and `llms.txt`. The browser needs no framework, package install,
or external service. Without JavaScript, every page remains readable.

## Maintain the content

The home page text lives in [build.py](build.py) (`home()` and `ADDS`). Examples
live in [recipes.py](recipes.py). Add an example only when it shows something a
reader would care about before trying Archguard. See [ux.md](ux.md) for visual
and writing rules.

Code blocks accept `json`, `shell`, `typescript`, `javascript`, `rust`, `text`,
and `output`. The builder escapes all code and highlights tokens at build time.
Use the `policy` helper for rule examples. Its failing, fixed, and control file
sets drive the CLI checks, and its `report` must be the CLI's real text output.

Run from the repository root:

```sh
cargo build --locked --bins --examples
python3 website/check.py
```

The check builds ignored `website/dist/` and audits links, anchors, code copy
text, metadata, agent files, and font hashes. It checks that every rule kind is
documented in the reference that `llms.txt` links. It then runs each policy
example's failure, fix, and negative control, and compares the shown report
with the real output. The repository's [complete check](../scripts/check.sh)
includes this check.

## Publish

The isolated `gh-pages` branch contains generated files only, with `.nojekyll`.
GitHub Pages uses that branch's root. There is no added CI workflow.

After independent review, complete validation, and authorized merge, check out
the merged `main` revision in a clean checkout with the CLI built, then run:

```sh
python3 website/publish.py
```

The publisher checks repository identity and the remote `main` revision, runs
the website checks, creates an isolated temporary Git checkout, preserves the
existing deployment history, and pushes without force. A concurrent deployment
causes a normal push rejection. It never changes the working branch or Pages
settings. `deployment.json` records the exact source revision.

For initial setup only, create the repository Pages site with source branch
`gh-pages` and source path `/` through `gh api`. Publishing and repository
settings require operator authorization. Verify the Pages build status and the
public index, nested example, CSS, JavaScript, font, `llms.txt`, JSON, and Markdown
URLs before reporting a deployment complete.

The canonical project prefix is `/irudd-ts/`. Assets and navigation use relative
URLs at each page depth. If the repository or hosting location changes, update
`SITE` and `REPO` in [build.py](build.py), the publisher target, and this guide.
Preview and check the project prefix as well as nested pages.

## Fonts and license

IBM Plex Sans and Mono are self-hosted, unmodified WOFF2 files from
[IBM/plex revision 763c36ef9117782905ae010056dfbe8fd2653a25](https://github.com/IBM/plex/tree/763c36ef9117782905ae010056dfbe8fd2653a25).
Sans uses `packages/plex-sans/fonts/complete/woff2/`; Mono uses
`packages/plex-mono/fonts/complete/woff2/`. The upstream copyright notice and
SIL Open Font License 1.1 are retained in [OFL.txt](assets/fonts/OFL.txt).
[The inventory](assets/fonts/inventory.json) pins their hashes and source revision.
Retain the notice, license, and inventory when updating the fonts. No other
third-party website assets or runtime dependencies are bundled.
