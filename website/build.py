#!/usr/bin/env python3
"""Build the static cookbook with Python's standard library."""
import html
import json
from pathlib import Path
import re
import shutil

from recipes import CATEGORIES, RECIPES, block

ROOT = Path(__file__).resolve().parent
DIST = ROOT / "dist"
SITE = "https://alundgren.github.io/irudd-ts/"
REPO = "https://github.com/alundgren/irudd-ts/blob/main/"


def esc(value):
    return html.escape(str(value), quote=True)


TOKEN = re.compile(r'(?P<comment>//[^\n]*|/\*[\s\S]*?\*/|\#[^\n]*)|'
                   r'(?P<string>"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|`(?:\\.|[^`\\])*`)|'
                   r'(?P<number>\b\d+(?:\.\d+)?\b)|(?P<word>\b[A-Za-z_$][\w$]*\b)|'
                   r'(?P<operator>[{}\[\]:,=<>+*?!|&;]+)')
KEYWORDS = set("import from export const let var function return if else for of in new class extends type interface async await true false null undefined use pub fn mod struct impl enum match mut self println console cargo archguard node npm cp mkdir rm echo".split())


def highlight(code, language):
    """Escape every byte of text; decorate lexical tokens without changing copy text."""
    if language == "text":
        # Prompt variables and numeric budgets still get restrained highlighting.
        return re.sub(r'(\$[\w-]+|\b\d+\b)', r'<span class="token-keyword">\1</span>', esc(code))
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
    return f'''<section class="code-section"><div class="code-label"><h3>{esc(item['label'])}</h3>
<button class="copy" type="button" data-copy="code-{index}" hidden>Copy</button></div>
<pre tabindex="0"><code id="code-{index}" class="language-{item['language']}">{highlight(item['code'], item['language'])}</code></pre></section>'''


def diagram(nodes, caption, connected=True):
    """Authored file/result diagram grammar. Labels remain real selectable text."""
    width, gap, box = 760, 30, (760 - (len(nodes) - 1) * 30) / len(nodes)
    labels = []
    for index, (label, state) in enumerate(nodes):
        x = index * (box + gap)
        # Split long labels at word boundaries, with at most three compact lines.
        words, lines = label.split(), [""]
        for word in words:
            if len(lines[-1]) + len(word) > max(18, int(box / 9)):
                lines.append(word)
            else:
                lines[-1] += (" " if lines[-1] else "") + word
        text = "".join(f'<tspan x="{x + box / 2}" y="{51 + (j - (len(lines)-1)/2) * 21}">{esc(line)}</tspan>' for j, line in enumerate(lines))
        labels.append(f'<g class="diagram-{state}"><rect x="{x}" y="12" width="{box}" height="78" rx="12"/><text text-anchor="middle">{text}</text></g>')
        if connected and index < len(nodes) - 1:
            start = x + box + 5
            labels.append(f'<path class="diagram-arrow" d="M {start} 51 H {start+18} m -6 -5 l 6 5 l -6 5"/>')
    # Mobile keeps labels readable with a separate vertical layout of the same facts.
    mobile = ''.join(f'<li class="diagram-{state}">{esc(label)}</li>' for label, state in nodes)
    return f'''<figure class="diagram{' disconnected' if not connected else ''}"><svg viewBox="0 0 {width} 102" role="img" aria-label="{esc(caption)}">{''.join(labels)}</svg>
<ol class="diagram-mobile" aria-hidden="true">{mobile}</ol><figcaption>{esc(caption)}</figcaption></figure>'''


def public_entry_diagram():
    caption = "A consumer can import @app/api. An import directly into private source is blocked."
    return f'''<figure class="diagram alternatives"><svg viewBox="0 0 760 216" role="img" aria-label="{caption}">
<g class="diagram-info"><rect x="0" y="69" width="210" height="78" rx="12"/><text x="105" y="114" text-anchor="middle">Consumer</text></g>
<path class="diagram-arrow" d="M 216 108 H 300 V 51 H 382 m -6 -5 l 6 5 l -6 5 M 300 108 V 165 H 382 m -6 -5 l 6 5 l -6 5"/>
<g class="diagram-ok"><rect x="390" y="12" width="370" height="78" rx="12"/><text x="575" y="57" text-anchor="middle">@app/api · allowed</text></g>
<g class="diagram-danger"><rect x="390" y="126" width="370" height="78" rx="12"/><text x="575" y="171" text-anchor="middle">Private source path · blocked</text></g></svg>
<ol class="diagram-mobile" aria-hidden="true"><li class="diagram-info">Consumer</li><li class="diagram-ok">@app/api · allowed</li><li class="diagram-danger">Private source path · blocked</li></ol>
<figcaption>{caption}</figcaption></figure>'''


def layout(title, description, body, prefix="", path="", *, extra_head=""):
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{esc(title)} · Archguard recipes</title><meta name="description" content="{esc(description)}">
<link rel="canonical" href="{SITE}{path}"><meta name="theme-color" content="#F2EADE">
<link rel="icon" href="{prefix}assets/mark.svg" type="image/svg+xml">
<link rel="stylesheet" href="{prefix}assets/site.css"><script src="{prefix}assets/site.js" defer></script>{extra_head}</head>
<body><a class="skip-link" href="#main">Skip to content</a>
<header class="site-header"><a class="brand" href="{prefix}index.html"><img src="{prefix}assets/mark.svg" width="28" height="28" alt="">Archguard<span>recipe book</span></a>
<nav aria-label="Main"><a href="{prefix}index.html#recipes">Recipes</a><a href="{prefix}agents.html">For agents</a><a href="https://github.com/alundgren/irudd-ts">GitHub <span aria-hidden="true">↗</span></a></nav></header>
<main id="main">{body}</main>
<footer class="site-footer"><span>Archguard · explicit repository checks</span><div><a href="{prefix}style.html">Visual style</a><a href="{prefix}llms.txt">Agent index</a><a href="{prefix}assets/fonts/OFL.txt">Font license</a></div></footer>
<p id="copy-status" class="visually-hidden" role="status" aria-live="polite"></p></body></html>'''


def recipe_page(recipe):
    slug = recipe["slug"]
    sources = ''.join(f'<a href="{REPO}{ref}">{esc(ref)}</a>' for ref in recipe["references"])
    related = ''.join(f'<a href="../{other}/index.html">{esc(next(r["title"] for r in RECIPES if r["slug"] == other))} <span aria-hidden="true">→</span></a>' for other in recipe["related"])
    flow_caption = recipe.get("caption", " → ".join(n[0] for n in recipe["nodes"]))
    visual = public_entry_diagram() if slug == "public-entry" else diagram(recipe['nodes'], flow_caption, slug not in {'read-results', 'duplicate-groups', 'classify-files', 'unique-service-id'})
    body = f'''<article class="recipe"><div class="recipe-top"><a class="back-link" data-back-to-results href="../../index.html#recipes">← All recipes</a><a href="index.md">Read as Markdown</a></div>
<p class="eyebrow">{esc(CATEGORIES[recipe['category']][0])}</p><h1>{esc(recipe['title'])}</h1><p class="lede">{esc(recipe['summary'])}</p>
<p class="needs"><span>You need</span> {esc(recipe['needs'])}</p>
{visual}
<div class="recipe-steps" id="example">{''.join(code_block(b, i) for i, b in enumerate(recipe['blocks']))}</div>
<section class="outcome" id="result"><h2>What to expect</h2><p>{esc(recipe['result'])}</p></section>
<aside class="limit" id="scope"><h2>Keep in mind</h2><p>{esc(recipe['boundary'])}</p></aside>
<details class="references"><summary>Reference and source</summary><div>{sources}</div></details>
<nav class="next-recipes" aria-label="Related recipes">{related}</nav></article>'''
    return layout(recipe["title"], recipe["summary"], body, "../../", f"recipes/{slug}/",
                  extra_head='<link rel="alternate" type="text/markdown" href="index.md">')


def markdown(recipe):
    text = f"# {recipe['title']}\n\n{recipe['summary']}\n\nCategory: {CATEGORIES[recipe['category']][0]}\nCapabilities: {', '.join(recipe['capabilities'])}\n\nYou need: {recipe['needs']}\n\n"
    text += "Diagram: " + recipe.get("caption", " → ".join(n[0] for n in recipe["nodes"])) + "\n\n"
    for item in recipe["blocks"]:
        text += f"## {item['label']}\n\n```{item['language']}\n{item['code']}\n```\n\n"
    text += f"## What to expect\n\n{recipe['result']}\n\n## Keep in mind\n\n{recipe['boundary']}\n\n## Reference and source\n\n"
    text += ''.join(f"- [{ref}]({REPO}{ref})\n" for ref in recipe["references"])
    text += "\n## Related recipes\n\n" + ''.join(f"- [{other}]({SITE}recipes/{other}/index.md)\n" for other in recipe["related"])
    return text


def home():
    groups = []
    for category, (name, description) in CATEGORIES.items():
        rows = []
        for recipe in RECIPES:
            if recipe["category"] != category:
                continue
            search = ' '.join([recipe['title'], recipe['summary'], recipe['boundary'], *recipe['capabilities'], *[b['code'] for b in recipe['blocks']]])
            rows.append(f'''<a class="recipe-row" data-recipe data-category="{category}" data-search="{esc(search.lower())}" href="recipes/{recipe['slug']}/index.html">
<div><h3>{esc(recipe['title'])}</h3><p>{esc(recipe['summary'])}</p></div><span class="row-arrow" aria-hidden="true">↗</span></a>''')
        groups.append(f'<section class="recipe-group" data-group="{category}" id="{category}"><div class="group-heading"><h2>{name}</h2><p>{description}</p></div>{"".join(rows)}</section>')
    filters = '<button type="button" data-category="all" aria-pressed="true">All recipes</button>' + ''.join(f'<button type="button" data-category="{key}" aria-pressed="false">{name}</button>' for key, (name, _) in CATEGORIES.items())
    body = f'''<section class="intro"><p class="eyebrow">A cookbook for your repository</p><h1>Make the rules of your<br class="wide-only"> codebase checkable.</h1>
<p class="lede">Keep dependencies in bounds. Check file conventions.<br class="wide-only"> Find duplicate code and test behavior with mutations.</p>
<a class="primary-link" href="recipes/first-check/index.html">Run your first check <span aria-hidden="true">→</span></a>
<a class="quiet-link" href="agents.html">Point your agent here <span aria-hidden="true">↗</span></a></section>
<details class="overview"><summary>How a check works</summary>{diagram([('Your source + config', 'neutral'), ('Archguard checks', 'info'), ('Findings with evidence', 'neutral')], 'Select sources, choose checks, and review findings.')}</details>
<section class="catalog" id="recipes"><div class="catalog-title"><h2>Find a recipe</h2><p>{len(RECIPES)} practical examples. Pick the task you have.</p></div>
<div class="search-tools" hidden><label for="recipe-search">Search recipes</label><div class="search-field"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="10" cy="10" r="6"/><path d="m15 15 6 6"/></svg><input id="recipe-search" type="search" placeholder="Try client imports, companion files, or mutations…" autocomplete="off" aria-describedby="search-help"><button id="clear-search" type="button" hidden aria-label="Clear search">Clear</button></div>
<p id="search-help">Search by task, rule name, command, or configuration key.</p><div class="filters" role="group" aria-label="Recipe categories">{filters}</div>
<p id="result-count" role="status" aria-live="polite"></p></div>
<noscript><p>Every recipe is listed below. Use your browser's Find command to search.</p></noscript>
<div id="recipe-list">{''.join(groups)}</div><div class="empty" id="empty-results" hidden><h3>No recipes match that search.</h3><p>Try a task such as imports, exports, or tests.</p><button type="button" id="reset-search">Show all recipes</button></div></section>'''
    return layout("Make your codebase rules checkable", "Search practical examples of every Archguard capability, with code, configurations, and diagrams.", body)


def agents_page():
    entries = ''.join(f'<li><a href="recipes/{r["slug"]}/index.md">{esc(r["title"])}</a> <span>{esc(", ".join(r["capabilities"]))}</span></li>' for r in RECIPES)
    body = f'''<article class="recipe"><p class="eyebrow">For coding agents</p><h1>Start with the task. Follow the source.</h1><p class="lede">The same recipes are available as static HTML, Markdown, and a JSON index. No browser automation required.</p>
<section class="agent-files"><a href="llms.txt">llms.txt <span>Task index and product contracts</span></a><a href="recipes.json">recipes.json <span>Capabilities, content, and source links</span></a><a href="https://github.com/alundgren/irudd-ts/blob/main/docs/README.md">Full reference <span>Configuration, protocols, and development</span></a></section>
{code_block(block('Give your agent this task', 'text', f'Read {SITE}llms.txt.\nFind the recipe for my task and follow its source links.\nPropose only checks supported by this repository\'s conventions.\nPreserve incomplete analysis and distinguish policy from compiler diagnostics.'), 0)}
<h2>Recipe map</h2><ul class="agent-index">{entries}</ul></article>'''
    return layout("For coding agents", "A static task map, Markdown recipes, JSON content, and links to Archguard source contracts.", body, path="agents.html")


def style_page():
    body = '''<article class="recipe"><p class="eyebrow">Visual and writing style</p><h1>Room to read. Color with a job.</h1><p class="lede">Warm paper, quiet type, and examples that show an action and its result.</p>
<h2>Color carries meaning</h2><div class="style-roles"><p class="style-neutral">Warm paper and soil ink organize the page.</p><p class="style-info">Blue identifies links, inputs to a check, and syntax keys.</p><p class="style-ok">Sage marks an allowed path or successful result.</p><p class="style-warn">Pear marks an incomplete result or a qualification.</p><p class="style-danger">Clay marks a blocked path or policy finding.</p></div>
<p>Every status also has a text label. Categories share one treatment. Color does not decorate category names.</p>
<h2>Examples do the explaining</h2><p>Name the file. Show the configuration, source, and command. Say what changes after the correction. Link to the detailed reference.</p>
<h2>Diagrams stay small</h2><p>Selectable labels, rounded file nodes, and visible arrows. Use two to four nodes for one relationship. On a phone the labels stack without shrinking.</p>
<h2>Type stays quiet</h2><p>Self-hosted IBM Plex Sans for prose. IBM Plex Mono for code and paths. Three weights, short line lengths, and generous space between examples.</p>
<h2>Write like a colleague</h2><p>Use task names and plain verbs. Keep prerequisites and evidence limits nearby. Leave justification and protocol details in the linked reference.</p>
<a href="https://github.com/alundgren/irudd-ts/blob/main/website/ux.md">Read the maintained style record ↗</a></article>'''
    return layout("Visual and writing style", "The maintained visual and writing style for the Archguard recipe book.", body, path="style.html")


def build():
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir()
    shutil.copytree(ROOT / "assets", DIST / "assets")
    for filename, content in [("index.html", home()), ("agents.html", agents_page()), ("style.html", style_page())]:
        (DIST / filename).write_text(content)
    for recipe in RECIPES:
        directory = DIST / "recipes" / recipe["slug"]
        directory.mkdir(parents=True)
        (directory / "index.html").write_text(recipe_page(recipe))
        (directory / "index.md").write_text(markdown(recipe))
    public = []
    for recipe in RECIPES:
        record = {key: value for key, value in recipe.items() if key != "fixture"}
        record.update(url=SITE + f"recipes/{recipe['slug']}/", markdown=SITE + f"recipes/{recipe['slug']}/index.md")
        record["references"] = [REPO + path for path in recipe["references"]]
        public.append(record)
    (DIST / "recipes.json").write_text(json.dumps({"schemaVersion": 1, "recipes": public}, indent=2) + "\n")
    index = "# Archguard recipe book\n\n> Practical examples of repository architecture checks, file conventions, compiler member checks, duplicate evidence, and mutation testing.\n\n"
    index += "## Contracts\n\nGraph facts, compiler facts, policy, and runtime behavior are distinct. Exit 0 requires complete clean checks; exit 1 means complete policy violations; exit 2 means incomplete analysis or invalid config. Dryer and mutator exit 0 for complete evidence, including matches and survivors. Execute only explicitly configured trusted commands. Selectors do not expand discovery.\n\n"
    index += f"## Maps\n\n- [JSON recipe index]({SITE}recipes.json)\n- [Full reference]({REPO}docs/README.md)\n- [Repository instructions]({REPO}AGENTS.md)\n\n"
    for key, (name, _) in CATEGORIES.items():
        index += f"## {name}\n\n"
        index += ''.join(f"- [{r['title']}]({SITE}recipes/{r['slug']}/index.md): {r['summary']}\n" for r in RECIPES if r["category"] == key)
        index += "\n"
    (DIST / "llms.txt").write_text(index)
    (DIST / ".nojekyll").touch()
    (DIST / "404.html").write_text(layout("Recipe not found", "Return to the searchable Archguard cookbook.", '<article class="recipe"><h1>That recipe is not here.</h1><p><a href="'+SITE+'">Find a recipe in the cookbook →</a></p></article>', prefix=SITE, path="404.html"))
    urls = [SITE, SITE + "agents.html", SITE + "style.html"] + [SITE + f"recipes/{r['slug']}/" for r in RECIPES]
    (DIST / "sitemap.xml").write_text('<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' + ''.join(f'<url><loc>{esc(url)}</loc></url>' for url in urls) + '</urlset>')
    (DIST / "robots.txt").write_text(f"User-agent: *\nAllow: /\nSitemap: {SITE}sitemap.xml\n")
    print(f"Built {len(RECIPES)} recipes in {DIST}")


if __name__ == "__main__":
    build()
