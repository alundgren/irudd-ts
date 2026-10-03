"""Observe an external pinned checkout without copying its implementation."""
import argparse
from collections import Counter
import importlib
import json
from pathlib import Path
import sys
from importlib.metadata import version

REVISION = "6892667b3441b88379bc8d0439fc2152b0fdb341"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--checkout", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("files", nargs="+")
    args = parser.parse_args()
    for package, expected in [("tree-sitter", "0.26.0"), ("tree-sitter-language-pack", "1.20.0")]:
        if version(package) != expected:
            raise ValueError(f"{package} must be {expected}")
    sys.path.insert(0, str(args.checkout / "src"))
    extract = importlib.import_module("dryer.extract")
    norm = importlib.import_module("dryer.astnorm")
    metric = importlib.import_module("dryer.shape")
    trees = importlib.import_module("dryer.treesitter")
    entries, excluded, diagnostics, parse_observations = [], [], [], []
    for relative in args.files:
        try:
            source = (args.root / relative).read_text(encoding="utf-8")
            data, tree = trees.parse(source, "tsx" if relative.endswith(".tsx") else "typescript")
            errors = [dict(type=n.type, start_line=n.start_point[0] + 1,
                           end_line=n.end_point[0] + 1, missing=n.is_missing)
                      for n in trees.descendants(tree.root_node) if n.is_error or n.is_missing]
            actual, warning = extract.entries_in_source("typescript", source, relative, relative)
            parse_observations.append(dict(file=relative, has_error=tree.root_node.has_error,
                                           errors=errors, upstream_warning=warning,
                                           upstream_entries=len(actual)))
            if errors or tree.root_node.has_error:
                diagnostics.append(dict(file=relative, errors=errors, message="tree-sitter parse incomplete"))
                continue
            for node in extract._typescript_nodes(data, tree.root_node):
                normalized = norm.normalize(node, data)
                counts, sizes = Counter(), {}
                def walk(item):
                    key = metric.pr(item)
                    counts[key] += 1
                    sizes[key] = metric.node_count(item)
                    if isinstance(item, list):
                        for child in item:
                            walk(child)
                walk(normalized)
                item = dict(file=relative, start=trees.start_line(node), end=trees.end_line(node),
                            nodes=metric.node_count(normalized))
                if item["end"] - item["start"] + 1 < 4 or item["nodes"] < 20:
                    excluded.append(dict(**item, reason="minimum size"))
                else:
                    entries.append(dict(**item, counts=counts, sizes=sizes))
        except Exception as error:
            diagnostics.append(dict(file=relative, message=f"{type(error).__name__}: {error}"))
    def location(item):
        return {key: item[key] for key in ["file", "start", "end", "nodes"]}
    pairs = []
    for index, left in enumerate(entries):
        for right in entries[index + 1:]:
            keys = left["counts"].keys() | right["counts"].keys()
            shared = sum(min(left["counts"][k], right["counts"][k]) for k in keys)
            union = sum(max(left["counts"][k], right["counts"][k]) for k in keys)
            weighted_shared = sum(min(left["counts"][k], right["counts"][k]) * left["sizes"].get(k, right["sizes"].get(k)) for k in keys)
            weighted_union = sum(max(left["counts"][k], right["counts"][k]) * left["sizes"].get(k, right["sizes"].get(k)) for k in keys)
            score = metric.jaccard(frozenset(left["counts"]), frozenset(right["counts"]))
            pairs.append(dict(left=location(left), right=location(right), set=score,
                              multiset=shared/union, weighted=weighted_shared/weighted_union,
                              candidate_at_082=score >= 0.82))
    pairs.sort(key=lambda item: -item["set"])
    output = dict(status="incomplete" if diagnostics else "complete", upstream_revision=REVISION,
                  config=dict(min_lines=4, min_nodes=20, threshold=0.82,
                              weighted="subtree node count times occurrence count"),
                  diagnostics=diagnostics, parse_observations=parse_observations,
                  entries=[location(item) for item in entries], excluded=excluded, pairs=pairs,
                  sensitivity=[dict(threshold=t, set=sum(p["set"] >= t for p in pairs),
                                    multiset=sum(p["multiset"] >= t for p in pairs),
                                    weighted=sum(p["weighted"] >= t for p in pairs))
                               for t in [0.7, 0.75, 0.8, 0.82, 0.85, 0.9, 0.95]])
    print(json.dumps(output, indent=2))
    return 2 if diagnostics else 0

if __name__ == "__main__":
    raise SystemExit(main())
