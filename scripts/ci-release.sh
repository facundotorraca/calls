#!/usr/bin/env bash
# Publishes a new version: bumps it, tags it and pushes. CI (.github/workflows/release.yml)
# builds, signs, notarizes and uploads it to apps.ftor.dev; installed apps update themselves.
#   usage: scripts/ci-release.sh [version]    (default: next patch, e.g. 0.1.0 -> 0.1.1)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
conf="apps/desktop/src-tauri/tauri.conf.json"
cd "$root"

if [ "$(git branch --show-current)" != main ]; then
  echo "error: releases go out from main." >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "error: commit or stash your changes first." >&2
  exit 1
fi
git pull --ff-only --quiet

current="$(plutil -extract version raw "$conf")"
version="${1:-$(awk -F. '{ print $1 "." $2 "." $3 + 1 }' <<<"$current")}"
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "error: v$version already exists." >&2
  exit 1
fi

sed -i '' "s/^  \"version\": \"$current\"/  \"version\": \"$version\"/" "$conf"
git commit --quiet -m "release v$version" -- "$conf"
git tag "v$version"
git push --quiet origin main "v$version"

echo "v$current -> v$version: CI is building and publishing it (~15 min)"
echo "  https://github.com/facundotorraca/calls/actions"
