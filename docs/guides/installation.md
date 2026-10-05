# Install a released binary

Archguard distributes native executables for Linux and macOS on x86_64 and arm64.
Linux archives require Ubuntu 22.04 or newer with glibc 2.35 and libgcc. macOS
archives require macOS 15 or newer. Each release builds and runs its installed
archive on the corresponding native GitHub runner. Other Linux distributions
with those libraries may work, but are not part of release smoke validation.

Choose an exact version from the [published releases](https://github.com/alundgren/irudd-ts/releases).
No binary release exists until the first manual release is published. Download
the installer from the matching tag, inspect it, and run it:

```sh
curl --fail --location https://raw.githubusercontent.com/alundgren/irudd-ts/v0.1.0/scripts/install.sh -o install-archguard.sh
bash install-archguard.sh 0.1.0 "$HOME/.local/share/archguard/0.1.0"
export PATH="$HOME/.local/share/archguard/0.1.0/bin:$PATH"
archguard --version
archguard check --root . --config archguard.json
```

The installer checks SHA256 and the executable's version before moving the
archive into a new directory. It requires Bash, curl, tar, and either sha256sum
or shasum. It refuses an existing destination. Select a new directory when
upgrading, then point PATH at that version. Checksums catch damaged downloads;
trust the repository and release author before executing a downloaded binary.

Each archive includes `bin/archguard`, the TypeScript SDK, the evaluation skill,
examples, guides, the optional compiler adapter and its lockfile, actual Cargo
dependency license notices, and a release identity record. It does not include
a compiler installation or `node_modules`. Structural checks need no Node or
Rust installation. SDK scripts and the optional compiler adapter need Node 24.
Install the adapter only when you explicitly configure semantic checks:

```sh
npm ci --prefix "$HOME/.local/share/archguard/0.1.0/providers/typescript7" --ignore-scripts --no-audit --no-fund
```

Keep that provider's LICENSE and NOTICE.txt when redistributing its installed
compiler packages. See [compiler setup](../../providers/typescript7/README.md).

## Check a consumer repository in GitHub Actions

The composite Action calls the same installer and then runs the consumer's
configuration. Pin both the Action revision and the binary version. The example
uses the first release tag, which becomes available when that release is published:

```yaml
name: Architecture
on: [pull_request]
jobs:
  check:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5
      - uses: alundgren/irudd-ts@v0.1.0 # Prefer the release commit SHA for immutable Action code.
        with:
          version: '0.1.0'
          config: archguard.json
```

The command keeps Archguard's exit codes: 0 for complete clean analysis, 1 for
complete policy violations, and 2 for incomplete analysis or invalid
configuration. GitHub displays either nonzero result as a failed step. Inputs
are passed as quoted arguments. Paths are relative to the consumer checkout.
The `directory` output exposes the installed SDK and adapter source for flows
that explicitly install and configure additional trusted tooling. For semantic
checks that need setup before the check, use the standalone installer first,
install the pinned provider, and run `bin/archguard` directly.
