#!/usr/bin/env python3
"""Publish a checked main revision to the isolated GitHub Pages branch."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from build import DIST, ROOT

PROJECT = ROOT.parent
REPOSITORY = "alundgren/irudd-ts"


def run(*args, cwd=PROJECT, capture=False):
    result = subprocess.run(args, cwd=cwd, text=True, check=True, capture_output=capture)
    return result.stdout.strip() if capture else None


def publish():
    # Publish only the reviewed revision, never an uncommitted preview.
    if run("git", "status", "--porcelain", capture=True):
        raise RuntimeError("Commit or remove local changes before publishing")
    revision = run("git", "rev-parse", "HEAD", capture=True)
    main = run("gh", "api", f"repos/{REPOSITORY}/git/ref/heads/main", "--jq", ".object.sha", capture=True)
    if revision != main:
        raise RuntimeError("Check out the reviewed main revision before publishing")
    origin = run("git", "remote", "get-url", "origin", capture=True)
    if origin.rstrip('/') not in {f"https://github.com/{REPOSITORY}", f"https://github.com/{REPOSITORY}.git", f"git@github.com:{REPOSITORY}.git"}:
        raise RuntimeError("Unexpected origin")
    run("python3", str(ROOT / "check.py"))
    (DIST / "deployment.json").write_text(json.dumps({"sourceRevision": revision, "repository": REPOSITORY}) + "\n")
    with tempfile.TemporaryDirectory(prefix="archguard-pages-publish-") as directory:
        checkout = Path(directory)
        run("git", "init", "--initial-branch=gh-pages", str(checkout))
        run("git", "remote", "add", "origin", origin, cwd=checkout)
        refs = run("git", "ls-remote", "--heads", "origin", "gh-pages", capture=True)
        if refs:
            run("git", "fetch", "--depth=1", "origin", "gh-pages", cwd=checkout)
            run("git", "reset", "--hard", "FETCH_HEAD", cwd=checkout)
            for path in checkout.iterdir():
                if path.name != ".git":
                    shutil.rmtree(path) if path.is_dir() else path.unlink()
        shutil.copytree(DIST, checkout, dirs_exist_ok=True)
        run("git", "add", "--all", cwd=checkout)
        run("git", "-c", "user.name=Archguard documentation", "-c", "user.email=archguard-pages@users.noreply.github.com", "commit", "-m", f"Publish cookbook from {revision[:12]}", cwd=checkout)
        run("git", "push", "origin", "HEAD:gh-pages", cwd=checkout)
    print(json.dumps({"publishedBranch": "gh-pages", "sourceRevision": revision}))


if __name__ == "__main__":
    publish()
