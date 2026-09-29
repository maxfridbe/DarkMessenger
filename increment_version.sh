#!/bin/bash
# Date-based versioning: YY.MMDD.## (UTC), e.g. 26.0929.01 is the first
# build of 29 Sep 2026, 26.0929.02 the second.
#
#   ./increment_version.sh             next version for today -> write it everywhere
#   ./increment_version.sh --next      only print the next unpublished version
#   ./increment_version.sh --apply V   write version V everywhere (used by CI)
#
# "Next" is one past the highest ## already published today (git tags
# v26.0929.NN); a plain local run also counts version.txt so repeated local
# runs keep going up. CI runs --next on every push to main, so each
# push publishes a new GitHub Release tagged v<version>; nothing needs to be
# committed. Locally it just keeps your builds labelled.
#
# Where the version goes:
#   version.txt                  26.0929.01                (display form, release tag)
#   Cargo.toml [package]         26.136.129+26.0929.01     (see below)
#   app/build.gradle             versionName 26.0929.01, versionCode 26092901
#
# Cargo needs plain semver, and cargo-apk turns major.minor.patch into the
# Android versionCode with each part limited to 0-255. So the semver core is
# YY . (N >> 8) . (N & 255) with N = day_of_year * 128 + ##, which increases
# with every build (up to 127 a day), and the readable version rides along as
# build metadata (it becomes the APK's versionName and the in-game label).
set -euo pipefail
cd "$(dirname "$0")"

# $1 = "tags" to count only published tags (CI), anything else also counts
# the local version.txt so repeated local runs keep incrementing.
next_version() {
    local today max=0 n local_v=""
    today="$(date -u +%y.%m%d)"
    [ "${1:-}" = tags ] || local_v="$(sed -n "s/^$today\.//p" version.txt 2>/dev/null)"
    for n in $(git tag -l "v$today.*" 2>/dev/null | sed "s/^v$today\.//") $local_v; do
        [[ "$n" =~ ^[0-9]+$ ]] && (( 10#$n > max )) && max=$((10#$n))
    done
    printf '%s.%02d\n' "$today" $((max + 1))
}

apply_version() {
    local v="$1" yy mm dd nn doy n semver code
    [[ "$v" =~ ^([0-9]{2})\.([0-9]{2})([0-9]{2})\.([0-9]{2,3})$ ]] || { echo "bad version '$v' (want YY.MMDD.##)" >&2; exit 1; }
    yy=$((10#${BASH_REMATCH[1]})); mm=${BASH_REMATCH[2]}; dd=${BASH_REMATCH[3]}; nn=$((10#${BASH_REMATCH[4]}))
    (( nn >= 1 && nn <= 127 )) || { echo "build number must be 1-127, got $nn" >&2; exit 1; }
    doy=$((10#$(date -u -d "20$(printf '%02d' $yy)-$mm-$dd" +%j)))
    n=$((doy * 128 + nn))
    semver="$yy.$((n >> 8)).$((n & 255))+$v"
    code="${yy}${mm}${dd}$(printf '%02d' $nn)"

    echo "$v" > version.txt
    sed -i "0,/^version = \".*\"/s//version = \"$semver\"/" Cargo.toml
    sed -i "s/versionName \".*\"/versionName \"$v\"/; s/versionCode [0-9]*/versionCode $code/" app/build.gradle
    echo "Version $v (cargo $semver, gradle versionCode $code)"
}

case "${1:-}" in
    --next) next_version tags ;;
    --apply) apply_version "${2:?usage: $0 --apply YY.MMDD.##}" ;;
    "") apply_version "$(next_version)" ;;
    *) echo "usage: $0 [--next | --apply YY.MMDD.##]" >&2; exit 1 ;;
esac
