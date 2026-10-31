#!/usr/bin/env bash
# Uploads the release CI just built to apps.ftor.dev (ftor's ADR 0010): the versioned files
# first, then releases.json (every version, what ftor's history lists), latest.json (what
# installed apps update from) and manifest.json (what ftor lists) last, so a run that dies
# half way never points anything at missing files.
# Runs in run-release after `tauri build`; R2_* and the build come from the steps before it.
#   usage: scripts/publish.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
conf="$root/apps/desktop/src-tauri/tauri.conf.json"
bundle="$root/target/release/bundle"

slug="calls"
name="$(plutil -extract productName raw "$conf")"
version="$(plutil -extract version raw "$conf")"
tagline="$(plutil -extract bundle.shortDescription raw "$conf")"
min_macos="$(plutil -extract bundle.macOS.minimumSystemVersion raw "$conf")"
dmg="$bundle/dmg/${name}_${version}_aarch64.dmg"
update="$bundle/macos/$name.app.tar.gz"
url="$R2_PUBLIC_URL/$slug"

# The commits since the previous release. run-release tags only after publishing, so the
# last tag is the previous version and HEAD is what is being released.
# The first one has no previous version to compare with.
if previous="$(git -C "$root" describe --tags --abbrev=0 2>/dev/null)"; then
  notes="$(git -C "$root" log --no-merges --format='- %s' "$previous..HEAD" | grep -v "^- release v" || true)"
else
  notes="First release."
fi

out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
sips -Z 256 "$root/apps/desktop/src-tauri/icons/icon.png" --out "$out/icon.png" >/dev/null

# Tauri's updater format: https://v2.tauri.app/plugin/updater/#static-json-file
jq -n --arg version "$version" --arg notes "$notes" --arg date "$(date -u +%FT%TZ)" \
  --arg signature "$(cat "$update.sig")" --arg url "$url/$version/$name.app.tar.gz" \
  '{version: $version, notes: $notes, pub_date: $date,
    platforms: {"darwin-aarch64": {signature: $signature, url: $url}}}' >"$out/latest.json"

jq -n --arg slug "$slug" --arg name "$name" --arg tagline "$tagline" --arg version "$version" \
  --arg date "$(date -u +%FT%TZ)" --arg notes "$notes" --arg min_macos "$min_macos" \
  --arg dmg_url "$url/$version/$(basename "$dmg")" --arg icon_url "$url/icon.png" \
  --argjson size "$(stat -f %z "$dmg")" \
  '{slug: $slug, name: $name, tagline: $tagline, version: $version, published_at: $date,
    notes: $notes, dmg_url: $dmg_url, icon_url: $icon_url, size_bytes: $size,
    min_macos: $min_macos}' >"$out/manifest.json"

export AWS_ACCESS_KEY_ID="$R2_ACCESS_KEY_ID" AWS_SECRET_ACCESS_KEY="$R2_SECRET_ACCESS_KEY"
export AWS_DEFAULT_REGION=auto
put() { aws s3 cp --endpoint-url "$R2_ENDPOINT" --only-show-errors "$@"; }
dest="s3://$R2_BUCKET/$slug"

# Copies an object from the bucket into a file, or returns 1 when there is no such object.
# Any other failure stops the release: "could not read the list" taken as "there is no list"
# would publish a history with every earlier version missing.
fetch() {
  local err
  if ! err="$(aws s3 cp --endpoint-url "$R2_ENDPOINT" --only-show-errors "$1" "$2" 2>&1)"; then
    case "$err" in
      *"(404)"* | *NoSuchKey*) return 1 ;;
      *) echo "$err" >&2 && exit 1 ;;
    esac
  fi
}

# Every release, newest first, each entry the manifest it shipped with. The bucket cannot be
# listed, so the history is a file this release goes on top of. The first one starts from the
# release manifest.json still points at, so what shipped before the file existed is kept; a
# version published again replaces its own entry. Read before anything is uploaded, so a list
# that is not one stops the run with the bucket untouched.
if fetch "$dest/releases.json" "$out/previous.json"; then
  :
elif fetch "$dest/manifest.json" "$out/current.json"; then
  jq '[.]' "$out/current.json" >"$out/previous.json"
else
  echo '[]' >"$out/previous.json"
fi
jq --slurpfile release "$out/manifest.json" \
  '$release + map(select(.version != $release[0].version))' "$out/previous.json" >"$out/releases.json"

# A version's files never change, so they can be cached forever; the pointers must not be.
forever="public, max-age=31536000, immutable"
fresh="no-cache"
put "$dmg" "$dest/$version/$(basename "$dmg")" --cache-control "$forever"
put "$update" "$dest/$version/$name.app.tar.gz" --cache-control "$forever"
put "$out/manifest.json" "$dest/$version/manifest.json" --cache-control "$forever" --content-type application/json
put "$out/icon.png" "$dest/icon.png" --cache-control "$fresh"
put "$dmg" "$dest/$name.dmg" --cache-control "$fresh"
put "$out/releases.json" "$dest/releases.json" --cache-control "$fresh" --content-type application/json
put "$out/latest.json" "$dest/latest.json" --cache-control "$fresh" --content-type application/json
put "$out/manifest.json" "$dest/manifest.json" --cache-control "$fresh" --content-type application/json

echo "published $name $version: $url/$name.dmg"
