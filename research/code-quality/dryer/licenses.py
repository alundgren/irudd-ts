"""Inventory the locked research installation and retain dependency notices."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent

def notice_text(data):
    # Preserve notice text; normalize line endings and trailing whitespace.
    return "\n".join(line.rstrip() for line in data.decode("utf-8", errors="replace").splitlines()).rstrip("\n")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    args = parser.parse_args()
    lock = json.loads((HERE / "package-lock.json").read_text())
    items, notices = [], []
    for location, item in sorted(lock["packages"].items()):
        if not location:
            continue
        installed = HERE / location
        metadata = json.loads((installed / "package.json").read_text())
        records = []
        for file in sorted(installed.iterdir()):
            if file.is_file() and (file.name.lower().startswith(("license", "licence", "copying")) or "notice" in file.name.lower()):
                data = file.read_bytes()
                records.append(dict(path=file.name, sha256=hashlib.sha256(data).hexdigest()))
                notices.append(f"===== {location}@{item['version']}/{file.name} =====\n" + notice_text(data))
        items.append(dict(path=location, name=metadata.get("name"), version=item["version"],
                          license=metadata.get("license", item.get("license", "UNDECLARED")),
                          integrity=item.get("integrity"), notices=records))
    site = Path(subprocess.check_output([args.python, "-c", "import sysconfig; print(sysconfig.get_path('purelib'))"], text=True).strip())
    for name, version in [("tree_sitter", "0.26.0"), ("tree_sitter_language_pack", "1.20.0")]:
        folder = site / f"{name}-{version}.dist-info"
        data = (folder / "licenses/LICENSE").read_bytes()
        notices.append(f"===== {name}@{version}/LICENSE =====\n" + notice_text(data))
        items.append(dict(name=name, version=version, license="MIT",
                          notices=[dict(path="licenses/LICENSE", sha256=hashlib.sha256(data).hexdigest())]))
    if any(item["license"] == "UNDECLARED" or not item["notices"] for item in items):
        raise ValueError("An installed dependency needs license review")
    (HERE / "dependency-licenses.json").write_text(json.dumps(items, indent=2) + "\n")
    (HERE / "DEPENDENCY-NOTICES.txt").write_text("\n\n".join(notices) + "\n")

if __name__ == "__main__":
    main()
