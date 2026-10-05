#!/usr/bin/env python3
"""Build the static site with Python's standard library."""
import html
import json
from pathlib import Path
import re
import shutil

from recipes import RECIPES, block

ROOT = Path(__file__).resolve().parent
DIST = ROOT / "dist"
SITE = "https://alundgren.github.io/irudd-ts/"
REPO = "https://github.com/alundgren/irudd-ts/blob/main/"
GITHUB = "https://github.com/alundgren/irudd-ts"


def esc(value):
    return html.escape(str(value), quote=True)


TOKEN = re.compile(r'(?P<comment>//[^\n]*|/\*[\s\S]*?\*/|\#[^\n]*)|'
                   r'(?P<string>"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|`(?:\\.|[^`\\])*`)|'
                   r'(?P<number>\b\d+(?:\.\d+)?\b)|(?P<word>\b[A-Za-z_$][\w$]*\b)|'
                   r'(?P<operator>[{}\[\]:,=<>+*?!|&;]+)')
KEYWORDS = set("import from export const let var function return if else for of in new class extends type interface async await true false null undefined use pub fn mod struct impl enum match mut self println console cargo archguard node npm cp mkdir rm echo".split())
# Terminal output: location, rule id, and outcome words.
OUTPUT = re.compile(r'^(?P<path>\S+:\d+(?::\d+)?:?)(?P<rule> [\w-]+:)?|(?P<survived>\bSurvived\b)', re.M)


def highlight(code, language):
    """Escape every byte of text; decorate lexical tokens without changing copy text."""
    if language == "text":
        # Prompt variables and numeric budgets still get restrained highlighting.
        return re.sub(r'(\$[\w-]+|\b\d+\b)', r'<span class="token-keyword">\1</span>', esc(code))
    if language == "output":
        def decorate(match):
            if match.group("survived"):
                return f'<span class="token-warn">{esc(match.group())}</span>'
            rule = match.group("rule") or ""
            return f'<span class="token-comment">{esc(match.group("path"))}</span>' + (f'<span class="token-danger">{esc(rule)}</span>' if rule else "")
        output, cursor = [], 0
        for match in OUTPUT.finditer(code):
            output.append(esc(code[cursor:match.start()]) + decorate(match))
            cursor = match.end()
        return "".join(output) + esc(code[cursor:])
    output, cursor = [], 0
    for token in TOKEN.finditer(code):
        output.append(esc(code[cursor:token.start()]))
        kind, value = token.lastgroup, token.group()
        if kind == "word":
            kind = "keyword" if value in KEYWORDS else None
        if kind == "string" and language == "json" and re.match(r'\s*:', code[token.end():]):
            kind = "key"
        output.append(f'<span class="token-{kind}">{esc(value)}</span>' if kind else esc(value))
        cursor = token.end()
    return "".join(output) + esc(code[cursor:])


def code_block(item, index):
    return f'''<div class="code-section"><div class="code-label"><p>{esc(item['label'])}</p>
<button class="copy" type="button" data-copy="code-{index}" hidden>Copy</button></div>
<pre tabindex="0"><code id="code-{index}" class="language-{item['language']}">{highlight(item['code'], item['language'])}</code></pre></div>'''


def diagram(nodes, caption, connected=True, *, visible_caption=True):
    """Authored node diagram. Labels remain real selectable text."""
    width, gap = 760, 30
    box = (width - (len(nodes) - 1) * gap) / len(nodes)
    labels = []
    for index, (label, state) in enumerate(nodes):
        x = index * (box + gap)
        # Split long labels at word boundaries.
        lines = [""]
        for word in label.split():
            if lines[-1] and len(lines[-1]) + len(word) > max(18, int(box / 9)):
                lines.append(word)
            else:
                lines[-1] += (" " if lines[-1] else "") + word
        text = "".join(f'<tspan x="{x + box / 2}" y="{57 + (j - (len(lines)-1)/2) * 21}">{esc(line)}</tspan>' for j, line in enumerate(lines))
        labels.append(f'<g class="diagram-{state}"><rect x="{x}" y="12" width="{box}" height="78" rx="12"/><text text-anchor="middle">{text}</text></g>')
        if connected and index < len(nodes) - 1:
            start = x + box + 5
            labels.append(f'<path class="diagram-arrow" d="M {start} 51 H {start+18} m -6 -5 l 6 5 l -6 5"/>')
    # Phones get the same labels stacked in reading order.
    mobile = ''.join(f'<li class="diagram-{state}">{esc(label)}</li>' for label, state in nodes)
    return f'''<figure class="diagram{'' if connected else ' disconnected'}"><svg viewBox="0 0 {width} 102" role="img" aria-label="{esc(caption)}">{''.join(labels)}</svg>
<ol class="diagram-mobile">{mobile}</ol>{f'<figcaption>{esc(caption)}</figcaption>' if visible_caption else ''}</figure>'''


def path_diagram():
    """Illustrate a configured transitive dependency restriction."""
    caption = "This example restricts dependencies from client to server, including paths through shared modules."
    nodes = [(0, "src/client/ui.ts", "info"), (350, "src/shared/format.ts", "neutral"), (700, "src/server/db.ts", "neutral")]
    parts = [f'<g class="diagram-{state}"><rect x="{x}" y="20" width="260" height="72" rx="12"/><text x="{x + 130}" y="62" text-anchor="middle">{label}</text></g>' for x, label, state in nodes]
    for start in (266, 616):
        parts.append(f'<path class="diagram-arrow" d="M {start} 56 H {start + 76} m -7 -6 l 7 6 l -7 6"/>')
        parts.append(f'<text class="diagram-note-ok" x="{start + 39}" y="122" text-anchor="middle">✓ allowed alone</text>')
    parts.append('<path class="diagram-path" d="M 130 98 V 160 Q 130 172 142 172 H 818 Q 830 172 830 160 V 98"/>')
    parts.append('<text class="diagram-note-danger" x="480" y="208" text-anchor="middle">✕ client reaches server · flagged</text>')
    return f'''<figure class="diagram hero-diagram"><svg viewBox="0 0 960 222" role="img" aria-label="{caption}">{''.join(parts)}</svg>
<ol class="diagram-mobile"><li class="diagram-info">src/client/ui.ts</li><li class="diagram-neutral">src/shared/format.ts <small>✓ allowed alone</small></li><li class="diagram-neutral">src/server/db.ts <small>✓ allowed alone</small></li><li class="diagram-danger">Whole path: client reaches server · flagged</li></ol>
<figcaption>{caption}</figcaption></figure>'''


def layout(title, description, body, prefix="", path="", *, extra_head=""):
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)} · Archguard</title><meta name="description" content="{esc(description)}">
<link rel="canonical" href="{SITE}{path}"><meta name="theme-color" content="#F2EADE">
<link rel="icon" href="{prefix}assets/mark.svg" type="image/svg+xml">
<link rel="stylesheet" href="{prefix}assets/site.css"><script src="{prefix}assets/site.js" defer></script>{extra_head}</head>
<body><a class="skip-link" href="#main">Skip to content</a>
<header class="site-header"><a class="brand" href="{prefix}index.html"><img src="{prefix}assets/mark.svg" width="28" height="28" alt="">Archguard</a>
<nav aria-label="Main"><a href="{prefix}index.html#examples">Examples</a><a href="{prefix}agents.html">For agents</a><a href="{GITHUB}">GitHub <span aria-hidden="true">↗</span></a></nav></header>
<main id="main">{body}</main>
<footer class="site-footer"><span>Archguard · MIT licensed</span><div><a href="{prefix}style.html">Site style</a><a href="{prefix}llms.txt">llms.txt</a><a href="{prefix}assets/fonts/OFL.txt">Font license</a></div></footer>
<p id="copy-status" class="visually-hidden" role="status" aria-live="polite"></p></body></html>'''


def recipe_page(recipe):
    slug = recipe["slug"]
    body = f'''<article class="recipe"><a class="back-link" data-back-to-results href="../../index.html#examples">← All examples</a>
<h1>{esc(recipe['title'])}</h1><p class="lede">{esc(recipe['summary'])}</p>
{diagram(recipe['nodes'], " → ".join(n[0] for n in recipe["nodes"]), visible_caption=False)}
<div class="recipe-steps">{''.join(code_block(b, i) for i, b in enumerate(recipe['blocks']))}</div>
<p class="outcome">{esc(recipe['result'])}</p>
<p class="more"><a href="{REPO}{recipe['reference']}">All options in the reference <span aria-hidden="true">↗</span></a></p></article>'''
    return layout(recipe["title"], recipe["summary"], body, "../../", f"examples/{slug}/",
                  extra_head='<link rel="alternate" type="text/markdown" href="index.md">')


def markdown(recipe):
    text = f"# {recipe['title']}\n\n{recipe['summary']}\n\n"
    for item in recipe["blocks"]:
        fence = "text" if item["language"] == "output" else item["language"]
        text += f"{item['label']}:\n\n```{fence}\n{item['code']}\n```\n\n"
    text += f"{recipe['result']}\n\nReference: {REPO}{recipe['reference']}\n"
    return text


def example_link(slug):
    recipe = next(r for r in RECIPES if r["slug"] == slug)
    return f'<a href="examples/{slug}/index.html">{esc(recipe["title"])}</a>'


ADDS = [
    ("Dependency paths", "Check configured dependency restrictions across intermediate modules.", ["block-server"]),
    ("Package imports and dependencies", "Require declared package entry points and check selected package dependencies.", ["public-entry", "package-dependencies"]),
    ("Specified calls", "Report specified calls in analyzed source or reachable dependencies.", ["block-calls"]),
    ("File conventions", "Check file roles, required companion files, and static registry imports.", ["classify-files", "companions", "registry-imports"]),
    ("Custom policies", "Write rules in TypeScript or Rust over the analyzed source graph.", ["typescript-plugin", "service-namespace"]),
    ("Duplication and mutation experiments", "Review structurally similar functions and edits that survive the tests you select.", ["run-mutations", "find-duplicates"]),
]

PROMPT = next(b["code"] for r in RECIPES if r["slug"] == "agent-evaluation" for b in r["blocks"])


def home():
    adds = ''.join(f'<li><h3>{esc(name)}</h3><p>{esc(text)}</p><p class="see">{" · ".join(example_link(s) for s in slugs)}</p></li>' for name, text, slugs in ADDS)
    rows = []
    for recipe in RECIPES:
        search = ' '.join([recipe['title'], recipe['summary'], recipe['result'], *recipe['capabilities'], *[b['code'] for b in recipe['blocks']]])
        rows.append(f'''<a class="recipe-row" data-recipe data-search="{esc(search.lower())}" href="examples/{recipe['slug']}/index.html">
<div><h3>{esc(recipe['title'])}</h3><p>{esc(recipe['summary'])}</p></div><span class="row-arrow" aria-hidden="true">→</span></a>''')
    body = f'''<section class="intro"><h1>Check dependency paths and repository conventions</h1>
<p class="lede">Archguard builds a source graph for the files you select and applies configured rules. It may be useful when your team repeatedly reviews the same import relationships or file conventions.</p>
{path_diagram()}
<p class="actions"><a class="primary-link" href="#try">Evaluate on your repository <span aria-hidden="true">→</span></a><a class="quiet-link" href="#examples">Read examples</a></p></section>

<section class="band" id="compare"><h2>Compare with your existing checks</h2>
<div class="compare"><div class="keep"><p class="column-title">Coverage to review in your current tools</p><ul>
<li>Correctness and style rules inside each file</li><li>Banning an import by name (<code>no-restricted-imports</code>)</li><li>Import cycles (<code>import/no-cycle</code>)</li><li>Type-aware rules</li></ul>
<p class="aside">Coverage depends on the rules and plugins you enable. Check your current configuration before adding overlapping rules.</p></div>
<div><p class="column-title">Archguard examples to evaluate</p><ul class="adds">{adds}</ul></div></div>
<p class="aside wide">If you use Oxlint or a dependency-analysis tool, compare these examples with what it already checks. Which policies are useful depends on your repository's conventions.</p></section>

<section class="band" id="trust"><h2>Check command status</h2>
<p><code>archguard check</code> returns 0 when the selected analysis is complete and configured policies pass. Parse errors and required import-resolution failures produce an incomplete result.</p>
{diagram([("0 · complete, no policy findings", "ok"), ("1 · complete, policy findings", "danger"), ("2 · incomplete or invalid configuration", "warn")], "Check command exit codes: 0 complete and clean, 1 complete with policy findings, 2 incomplete analysis or invalid configuration.", connected=False)}</section>

<section class="band" id="mutation-testing"><h2>Mutation testing</h2>
<p>Mutation testing checks your tests by changing the code they run. Archguard first runs your test command against unchanged source, then makes one small edit in an isolated copy and runs the tests again.</p>
{diagram([("Passing baseline", "ok"), ("One edit in a copy", "info"), ("Run the same tests", "neutral")], "Mutation testing: establish a passing baseline, make one isolated edit, then rerun the same tests.")}
<p>For example, changing <code>subtotal &gt;= 100</code> to <code>subtotal &gt; 100</code> changes what happens at 100. An assertion failure shows your tests caught the edit. If they still pass, that boundary may need an assertion. Review surviving edits before adding tests; some preserve behavior.</p>
<p class="more"><a href="examples/run-mutations/index.html">See a mutation test and its report <span aria-hidden="true">↗</span></a></p></section>

<section class="band" id="try"><h2>Evaluate a few checks on your repository</h2>
<p>An agent can try checks within an agreed time and disk budget. Review the proposed rules, findings, and reported limits before deciding which checks to keep.</p>
{diagram(next(r["nodes"] for r in RECIPES if r["slug"] == "agent-evaluation"), "You set the budget. The agent reports findings and, if you allow it, opens a draft PR.")}
{code_block(block("Paste this to your agent", "text", PROMPT), "prompt")}
<p class="more">For setup instructions, read the <a href="{GITHUB}#readme">README</a>. Agents can start with <a href="llms.txt">llms.txt</a>.</p></section>

<section class="catalog" id="examples"><h2>Examples</h2>
<div class="search-tools" hidden><label class="visually-hidden" for="recipe-search">Search examples</label><div class="search-field"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/></svg><input id="recipe-search" type="search" placeholder="Search: server, package, migration, tests…" autocomplete="off"><button id="clear-search" type="button" hidden aria-label="Clear search">Clear</button></div>
<p id="result-count" role="status" aria-live="polite"></p></div>
<div id="recipe-list">{''.join(rows)}</div><div class="empty" id="empty-results" hidden><p>No examples match that search.</p><button type="button" id="reset-search">Show all examples</button></div></section>'''
    return layout("Dependency and repository checks", "Configured checks over selected source dependencies and file conventions, plus duplication review and mutation testing.", body)


def agents_page():
    entries = ''.join(f'<li><a href="examples/{r["slug"]}/index.md">{esc(r["title"])}</a></li>' for r in RECIPES)
    body = f'''<article class="recipe"><h1>For coding agents</h1><p class="lede">Each example has an HTML page, a Markdown copy, and an entry in examples.json. Start with llms.txt. The full reference lives in the repository.</p>
<ul class="agent-files"><li><a href="llms.txt">llms.txt</a> What Archguard does, its contracts, and every example</li><li><a href="examples.json">examples.json</a> The examples as data</li><li><a href="{REPO}docs/README.md">Reference</a> Installation, configuration, every rule, plugins</li></ul>
{code_block(block("Paste this to your agent", "text", PROMPT), 0)}
<h2>Examples as Markdown</h2><ul class="agent-index">{entries}</ul></article>'''
    return layout("For coding agents", "Markdown copies, a JSON index, and links to the Archguard reference.", body, path="agents.html")


def style_page():
    body = '''<article class="recipe"><h1>Site style</h1><p class="lede">Warm paper, quiet type, and examples that show a problem and what Archguard says about it.</p>
<div class="style-roles"><p class="style-neutral">Warm paper and soil ink organize the page.</p><p class="style-info">Blue marks links and the starting file in a diagram.</p><p class="style-ok">Sage marks an allowed path or a pass.</p><p class="style-warn">Pear marks an incomplete result or a surviving mutation.</p><p class="style-danger">Clay marks a blocked path or a broken rule.</p></div>
<p>Color is only used when it carries one of those meanings. Every status also has a word or symbol, so color is never the only signal.</p>
<p>Describe the configured check, show a representative example, and name the scope of the result. Leave setup and protocol details to the reference.</p>
<p><a href="https://github.com/alundgren/irudd-ts/blob/main/website/ux.md">Read the full style record ↗</a></p></article>'''
    return layout("Site style", "The visual and writing style for the Archguard site.", body, path="style.html")


def llms():
    text = "# Archguard\n\n> Archguard applies configured policies to selected source files and their resolved dependencies. It also checks file conventions and provides duplication review and mutation testing tools. Use the examples as starting points and review their scope on your repository.\n\n"
    text += "## Compare existing coverage\n\nReview the rules and plugins already enabled in your linter and dependency-analysis tools. Archguard examples cover transitive dependency restrictions, package entry points and dependencies, specified calls, file roles, companion files, static registry imports, custom graph policies, structural similarity, and mutation testing. Some checks overlap with existing tools; evaluate which policies fit your repository's conventions.\n\n"
    text += "## Contracts\n\nFor archguard check, exit 0 means complete selected analysis with no policy findings. Exit 1 means complete analysis with rule violations. Exit 2 means incomplete analysis or invalid configuration. Selectors never add files to discovery. Archguard runs only plugin and provider commands that are explicitly configured. Duplicate and mutation reports exit 0 when complete, even with findings.\n\n"
    text += f"## Start\n\n- [Evaluate a repository with an agent]({REPO}docs/guides/evaluate.md): install the irudd-ts-evaluate skill and give it a time and disk budget.\n- [README: build and first check]({REPO}README.md)\n- [Full reference]({REPO}docs/README.md): configuration, file selection, import resolution, every rule kind, plugins, compiler member checks, caching.\n- [Every rule kind]({REPO}docs/guides/rules.md) and [repository rules]({REPO}docs/guides/repository-rules.md)\n- [Examples as JSON]({SITE}examples.json)\n\n## Examples\n\n"
    text += ''.join(f"- [{r['title']}]({SITE}examples/{r['slug']}/index.md): {r['summary']}\n" for r in RECIPES)
    return text


def build():
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir()
    shutil.copytree(ROOT / "assets", DIST / "assets")
    for filename, content in [("index.html", home()), ("agents.html", agents_page()), ("style.html", style_page())]:
        (DIST / filename).write_text(content)
    public = []
    for recipe in RECIPES:
        directory = DIST / "examples" / recipe["slug"]
        directory.mkdir(parents=True)
        (directory / "index.html").write_text(recipe_page(recipe))
        (directory / "index.md").write_text(markdown(recipe))
        record = {key: value for key, value in recipe.items() if key not in ("fixture", "nodes")}
        record.update(url=SITE + f"examples/{recipe['slug']}/", markdown=SITE + f"examples/{recipe['slug']}/index.md", reference=REPO + recipe["reference"])
        public.append(record)
    (DIST / "examples.json").write_text(json.dumps({"schemaVersion": 1, "examples": public}, indent=2) + "\n")
    (DIST / "llms.txt").write_text(llms())
    (DIST / ".nojekyll").touch()
    (DIST / "404.html").write_text(layout("Page not found", "Return to the Archguard site.", '<article class="recipe"><h1>That page is not here.</h1><p><a href="'+SITE+'">Go to the Archguard home page →</a></p></article>', prefix=SITE, path="404.html"))
    urls = [SITE, SITE + "agents.html", SITE + "style.html"] + [SITE + f"examples/{r['slug']}/" for r in RECIPES]
    (DIST / "sitemap.xml").write_text('<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' + ''.join(f'<url><loc>{esc(url)}</loc></url>' for url in urls) + '</urlset>')
    (DIST / "robots.txt").write_text(f"User-agent: *\nAllow: /\nSitemap: {SITE}sitemap.xml\n")
    print(f"Built {len(RECIPES)} examples in {DIST}")


if __name__ == "__main__":
    build()
