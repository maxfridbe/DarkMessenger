#!/bin/bash
# Bump the game version stored in version.txt and sync it everywhere it
# appears (Cargo.toml package version, Gradle versionName/versionCode).
#
#   ./increment_version.sh          -> bump patch  (0.1.0 -> 0.1.1)
#   ./increment_version.sh minor    -> bump minor  (0.1.1 -> 0.2.0)
#   ./increment_version.sh major    -> bump major  (0.2.0 -> 1.0.0)
#
# Commit the result; the release workflow (.github/workflows/release.yml)
# publishes a GitHub Release tagged v<version> on push to the main branch.
set -e
cd "$(dirname "$0")"

PART="${1:-patch}"
VER=$(cat version.txt)
IFS=. read -r MA MI PA <<< "$VER"

case "$PART" in
    major) MA=$((MA+1)); MI=0; PA=0;;
    minor) MI=$((MI+1)); PA=0;;
    patch) PA=$((PA+1));;
    *) echo "usage: $0 [major|minor|patch]"; exit 1;;
esac

NEW="$MA.$MI.$PA"
echo "$NEW" > version.txt

# Sync Cargo.toml [package] version (first version line in the file)
sed -i "0,/^version = \".*\"/s//version = \"$NEW\"/" Cargo.toml

# Sync the Gradle path (Path B): versionName + a monotonic versionCode
CODE=$((MA*10000 + MI*100 + PA))
sed -i "s/versionName \".*\"/versionName \"$NEW\"/" app/build.gradle
sed -i "s/versionCode [0-9]*/versionCode $CODE/" app/build.gradle

echo "Version: $VER -> $NEW (android versionCode $CODE)"
echo "Now commit + push; CI will publish release v$NEW."
