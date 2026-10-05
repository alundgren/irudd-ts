#!/usr/bin/env python3
"""Audit generated navigation and execute the recipe's policy examples."""
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.parse import unquote, urlsplit

from build import DIST, REPO, ROOT, SITE, build, highlight
from recipes import RECIPES

PROJECT = ROOT.parent


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__(convert_charrefs=True)
        self.ids, self.links, self.codes = set(), [], []
        self.language = None
        self.current = []
        self.feed(source)

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if "id" in attrs:
            assert attrs["id"] not in self.ids, f"Duplicate id {attrs['id']}"
            self.ids.add(attrs["id"])
        for name in ("href", "src"):
            if name in attrs:
                self.links.append(attrs[name])
        if tag == "code":
            self.language = attrs.get("class", "")
            self.current = []

    def handle_data(self, text):
        if self.language is not None:
            self.current.append(text)

    def handle_endtag(self, tag):
        if tag == "code":
            self.codes.append((self.language, ''.join(self.current)))
            self.language = None


def audit():
    slugs = [r["slug"] for r in RECIPES]
    assert len(slugs) == len(set(slugs)), "Duplicate recipe URL"
    languages = {"json", "shell", "typescript", "javascript", "rust", "text", "output"}
    for recipe in RECIPES:
        assert re.fullmatch(r"[a-z0-9-]+", recipe["slug"])
        assert all(recipe[key] for key in ("title", "summary", "nodes", "blocks", "result", "reference", "capabilities"))
        assert (PROJECT / recipe["reference"]).exists(), recipe["reference"]
        fixture = recipe["fixture"]
        for item in recipe["blocks"]:
            # Displayed project files must be the exact files the CLI check runs.
            if fixture and item["label"] in fixture["before"]:
                assert item["code"] == fixture["before"][item["label"]], item["label"]
            if fixture and item["label"].startswith("Fix · "):
                assert item["code"] == fixture["fixed"][item["label"][6:]], item["label"]
            assert item["language"] in languages
            if item["language"] == "json":
                json.loads(item["code"])
    pages = {path.resolve(): Page(path.read_text()) for path in DIST.rglob("*.html")}
    for path, page in pages.items():
        for href in page.links:
            if href.startswith(REPO):
                assert (PROJECT / unquote(href[len(REPO):].split('#')[0])).exists(), href
                continue
            if href.startswith(SITE):
                href = "/" + href[len(SITE):]
            url = urlsplit(href)
            if url.scheme or url.netloc:
                continue
            target = (DIST / unquote(url.path.lstrip('/'))) if url.path.startswith('/') else path.parent / unquote(url.path)
            if not url.path:
                target = path
            if target.is_dir():
                target /= "index.html"
            target = target.resolve()
            assert target.is_relative_to(DIST), href
            assert target.exists(), f"{path.relative_to(DIST)}: missing {href}"
            if url.fragment:
                assert target in pages and url.fragment in pages[target].ids, href
    for recipe in RECIPES:
        path = (DIST / "examples" / recipe["slug"] / "index.html").resolve()
        assert pages[path].codes == [(f"language-{b['language']}", b["code"]) for b in recipe["blocks"]], f"Changed code text: {recipe['slug']}"
        raw = path.read_text()
        assert '<meta name="description"' in raw and '<link rel="canonical"' in raw
        assert 'role="img"' in raw and 'type="text/markdown"' in raw
    # A code example containing HTML must remain text after highlighting.
    attack = 'const value = "</code><script>alert(1)</script>";'
    rendered = '<code class="language-typescript">' + highlight(attack, 'typescript') + '</code>'
    assert '<script>' not in rendered
    assert Page(rendered).codes[0][1] == attack
    index = json.loads((DIST / "examples.json").read_text())
    assert len(index["examples"]) == len(RECIPES)
    for record in index["examples"]:
        for link in (record["url"], record["markdown"]):
            assert link.startswith(SITE)
            target = DIST / link[len(SITE):]
            assert target.exists()
        assert record["markdown"] in (DIST / "llms.txt").read_text()
    # The site shows a selection. Agents reach every rule kind through the reference linked from llms.txt.
    config = (PROJECT / "src/config.rs").read_text()
    variants = re.search(r'pub enum RuleKind \{(.*?)\n\}', config, re.S).group(1)
    kinds = {name[0].lower() + name[1:] for name in re.findall(r'^\s*(\w+),', variants, re.M)}
    reference = (PROJECT / "docs/guides/rules.md").read_text() + (PROJECT / "docs/guides/repository-rules.md").read_text()
    assert all(f"`{kind}`" in reference for kind in kinds), f"Undocumented rule kinds: {[k for k in kinds if f'`{k}`' not in reference]}"
    llms = (DIST / "llms.txt").read_text()
    assert REPO + "docs/guides/rules.md" in llms and REPO + "docs/guides/repository-rules.md" in llms
    inventory = json.loads((ROOT / "assets/fonts/inventory.json").read_text())
    for name, digest in inventory["files"].items():
        assert hashlib.sha256((ROOT / "assets/fonts" / name).read_bytes()).hexdigest() == digest, name
    print(f"Audited {len(pages)} HTML pages, code text, navigation, rule reference coverage, agent files, and font inventory")


def policy_examples():
    binary = PROJECT / "target/debug/archguard"
    assert binary.is_file(), "Run cargo build --locked --bins --examples first"
    checked = 0
    for recipe in RECIPES:
        fixture = recipe["fixture"]
        if not fixture:
            continue
        for case, expected in [("before", 1), ("fixed", 0), ("control", 0)]:
            with tempfile.TemporaryDirectory(prefix="archguard-recipe-") as directory:
                root = Path(directory)
                for name, content in fixture[case].items():
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(content)
                config = root / "archguard.json"
                config.write_text(json.dumps(fixture["config"]))
                result = subprocess.run([str(binary), "check", "--root", str(root), "--config", str(config), "--json"], capture_output=True, text=True, timeout=20)
                if expected == 1:
                    # The page shows the real terminal report, minus the timing summary line.
                    text = subprocess.run([str(binary), "check", "--root", ".", "--config", "archguard.json"], cwd=root, capture_output=True, text=True, timeout=20)
                    assert text.stdout.rstrip("\n").rsplit("\n", 1)[0] == fixture["report"], f"{recipe['slug']}: shown report differs\n{text.stdout}"
                assert result.returncode == expected, f"{recipe['slug']} {case}: expected {expected}, got {result.returncode}\n{result.stdout}\n{result.stderr}"
                report = json.loads(result.stdout)
                assert report["complete"] and not report["problems"], f"{recipe['slug']} {case}: incomplete"
                if expected == 1:
                    assert report["diagnostics"] and all(d["rule"] == fixture["rule"] for d in report["diagnostics"])
                else:
                    assert not report["diagnostics"]
                checked += 1
    print(f"Executed {checked} example cases: policy failure with its shown report, published fix, and negative control")


if __name__ == "__main__":
    if sys.flags.optimize:
        raise SystemExit("Website checks require Python assertions enabled; remove -O and PYTHONOPTIMIZE")
    build()
    audit()
    policy_examples()
