#!/usr/bin/env bash
# Install one explicit published version. This is also the composite Action installer.
set -euo pipefail
trap 'exit 2' ERR
version=${1:?Usage: install.sh VERSION [INSTALL_DIRECTORY]}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Expected an exact version such as 0.1.0' >&2; exit 2; }
case "$(uname -s)" in Linux) os=linux;; Darwin) os=macos;; *) echo 'Supported systems: Linux and macOS' >&2; exit 2;; esac
case "$(uname -m)" in x86_64|amd64) arch=x86_64;; aarch64|arm64) arch=arm64;; *) echo 'Supported architectures: x86_64 and arm64' >&2; exit 2;; esac
destination=${2:-"${XDG_DATA_HOME:-$HOME/.local/share}/archguard/$version"}
[[ ! -e "$destination" ]] || { echo "Install directory already exists: $destination" >&2; exit 2; }
archive="archguard-$version-$os-$arch.tar.gz"
base=${ARCHGUARD_DOWNLOAD_BASE_URL:-"https://github.com/alundgren/irudd-ts/releases/download/v$version"}
mkdir -p "$(dirname "$destination")"
work=$(mktemp -d "$(dirname "$destination")/.archguard-install.XXXXXXXX")
trap 'rm -rf "$work"' EXIT
for asset in "$archive" "$archive.sha256"; do
  curl --fail --silent --show-error --location --proto '=https,file' "$base/$asset" -o "$work/$asset"
done
expected=$(awk -v name="$archive" 'NF == 2 && $2 == name {print $1}' "$work/$archive.sha256")
[[ "$expected" =~ ^[a-f0-9]{64}$ && $(wc -l < "$work/$archive.sha256") -eq 1 ]] || { echo 'Invalid archive checksum record' >&2; exit 2; }
if command -v sha256sum >/dev/null; then actual=$(sha256sum "$work/$archive" | cut -d ' ' -f 1); else actual=$(shasum -a 256 "$work/$archive" | cut -d ' ' -f 1); fi
[[ "$actual" == "$expected" ]] || { echo 'Archive checksum mismatch' >&2; exit 2; }
# The publisher creates a single top-level directory and regular files only.
root="archguard-$version"
tar -tzf "$work/$archive" > "$work/files"
awk -v root="$root" '$0 !~ ("^" root "(/|$)") || $0 ~ /(^|\/)\.\.(\/|$)/ {bad=1} END {exit bad}' "$work/files" || { echo 'Unsafe archive path' >&2; exit 2; }
tar -tvzf "$work/$archive" | awk 'substr($0, 1, 1) !~ /^[-d]$/ {bad=1} END {exit bad}' || { echo 'Archive contains unsupported file types' >&2; exit 2; }
tar -xzf "$work/$archive" -C "$work"
[[ "$(cat "$work/$root/VERSION")" == "$version" ]] || { echo 'Archive version mismatch' >&2; exit 2; }
[[ "$("$work/$root/bin/archguard" --version)" == "archguard $version" ]] || { echo 'Binary version mismatch' >&2; exit 2; }
mv "$work/$root" "$destination"
printf '%s\n' "$destination"
