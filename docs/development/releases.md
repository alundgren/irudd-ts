# Manual releases

Release work starts only when the operator asks for a release. Preparing a PR,
merging it, and publishing that requested version are part of that release task.
An unrelated packaging task does not authorize a release or repository settings
changes. Neither workflow runs on pushes, tags, PR events, or a schedule.

## One-time administrator prerequisite

The prepare workflow uses `GITHUB_TOKEN` to create release PRs. An administrator
must enable "Allow GitHub Actions to create and approve pull requests" before
the first requested release. Inspect the setting first:

```sh
gh api repos/alundgren/irudd-ts/actions/permissions/workflow
```

Only when the current release task authorizes this prerequisite, apply it:

```sh
gh api --method PUT repos/alundgren/irudd-ts/actions/permissions/workflow -F can_approve_pull_request_reviews=true
```

Read the setting again to verify it. Do not change it during packaging work.
The release agent checks this setting before dispatching preparation. The
workflow token cannot read this administrator endpoint; see the
[required API permission](https://docs.github.com/en/rest/actions/permissions#get-default-workflow-permissions-for-a-repository).
If the setting remains disabled, release-please cannot create its PR. Release permissions are confined to the two
manual workflows. There is no automatic validation CI in this repository.

## Agent release procedure

1. Inspect the last public release and all commits and diffs since its exact
   commit. For the first release inspect the complete project. Review changes
   to CLI flags, configuration, SDK exports, protocols, exit codes, platform
   requirements, and provider pins. Conventional Commit subjects and breaking
   footers can be incomplete. Identify incompatible behavior from the code too.
2. Determine the proposed next version. The first release is 0.1.0. While below
   1.0, breaking changes and features increase the minor version; fixes increase
   the patch. After 1.0, breaking changes increase the major version. Do not
   silently declare 1.0 stability. `release-please-config.json` records this
   policy. If commits omit a breaking change, record a Conventional Commit
   `BREAKING CHANGE:` footer before the source commit merges, or add a commit
   with release-please's documented `Release-As:` version override in its body
   on the default branch through a reviewed PR. Then regenerate preparation
   and verify its version and notes. An override must match the reviewed change
   scope. Editing only the release PR body does not set a version override.
3. Dispatch `prepare-release.yml` on the default branch with `gh workflow run`.
   Monitor the run. The pinned release-please Rust strategy creates or updates
   the release PR, Cargo versions, manifest and Conventional Commit rollup
   changelog. It never creates a GitHub release. An empty initial manifest
   selects 0.1.0; later manifests describe the last prepared version. Preparation
   refuses to proceed while that version is unpublished.
4. Fetch the release PR into a separate checkout. Compare the proposed version
   and changelog with the reviewed diffs. Correct missing breaking notes before
   merging. Run `python3 scripts/licenses.py --write` because the Cargo inventory
   includes Archguard's own version. Commit that inventory correction to the
   release PR, install the documented optional test environments, and run
   `scripts/check.sh`. Obtain independent review before merging. The operator's
   current release request authorizes only this release task.
5. Merge the reviewed release PR and record its exact merged commit. Dispatch
   `publish-release.yml` with only `release_pr=NUMBER`. The workflow derives the
   version and commit from that merged default-branch release PR. Never pass an
   arbitrary version or commit to publication. All four native builds, license
   packages, installer checks, and exit-code smoke tests must pass before a
   draft is created or resumed.
6. Monitor publication. The publisher rejects conflicting tags and asset bytes,
   verifies all four archives and all four checksum assets, and publishes only
   a complete draft. GitHub release notes come from the release-please changelog
   section. Do not use GitHub's automatic release notes. Verify the public tag,
   exact commit, notes and downloads. Only successful publication reconciles the
   release PR's `autorelease: pending` and `autorelease: tagged` labels.

Examples, run only as part of an explicitly requested release:

```sh
gh workflow run prepare-release.yml --repo alundgren/irudd-ts --ref main
gh run list --repo alundgren/irudd-ts --workflow prepare-release.yml
gh workflow run publish-release.yml --repo alundgren/irudd-ts --ref main -f release_pr=123
gh run list --repo alundgren/irudd-ts --workflow publish-release.yml
```

Use the actual default branch if it differs from `main`.

## Interrupted publication

Rerun publication with the same release PR number. The workflow uses the same
merged revision and fixed Rust 1.96.0 toolchain. The package writer normalizes
archive metadata. Existing matching draft assets remain in place; only missing
assets are uploaded. A conflicting tag, identity, checksum, note, or asset stops
the run. Inspect the cause instead of replacing conflicting bytes. If a draft
upload is interrupted, the release remains unpublished until complete.

After publication, reruns verify existing public assets and leave them intact.
They can finish release-label bookkeeping interrupted after publication.
Public assets are never deleted or replaced. There is no separate release
database; Cargo, the manifest, changelog, PR, tag, and GitHub release hold the
release state. Native runtime validation on all supported systems occurs during
the explicitly dispatched publisher; a local Linux check cannot establish macOS
or arm64 compatibility.
