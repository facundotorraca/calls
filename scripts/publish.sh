#!/usr/bin/env bash
# Uploads the release CI just built to apps.ftor.dev (ftor's ADR 0010): the versioned files
# first, then latest.json (what installed apps update from) and manifest.json (what ftor
# lists) last, so a run that dies half way never points anything at missing files.
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

# A version's files never change, so they can be cached forever; the pointers must not be.
forever="public, max-age=31536000, immutable"
fresh="no-cache"
put "$dmg" "$dest/$version/$(basename "$dmg")" --cache-control "$forever"
put "$update" "$dest/$version/$name.app.tar.gz" --cache-control "$forever"
put "$out/icon.png" "$dest/icon.png" --cache-control "$fresh"
put "$dmg" "$dest/$name.dmg" --cache-control "$fresh"
put "$out/latest.json" "$dest/latest.json" --cache-control "$fresh" --content-type application/json
put "$out/manifest.json" "$dest/manifest.json" --cache-control "$fresh" --content-type application/json

echo "published $name $version: $url/$name.dmg"
