#!/usr/bin/env bash
# Builds a .dmg to install Calls on other Macs.
#  - With a "Developer ID Application" certificate and `make notary`: signed and notarized
#    by Apple, it opens anywhere without warnings.
#  - Otherwise, signed with "Calls Local Signing" (`make signing`): the first time, the other
#    Mac has to allow it once (see the message at the end). Updates keep its permissions.
#   usage: scripts/release.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="calls-notary"
bundle="$root/target/release/bundle"
version="$(plutil -extract version raw "$root/apps/desktop/src-tauri/tauri.conf.json")"
dmg="$bundle/dmg/Calls_${version}_aarch64.dmg"

identities="$(security find-identity -v -p codesigning)"
identity="$(sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' <<<"$identities" | head -1)"
notarize=true
if [ -z "$identity" ] || ! xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1; then
  notarize=false
  identity="Calls Local Signing"
  if ! grep -q "\"$identity\"" <<<"$identities"; then
    echo "error: no signing identity, run 'make signing' (or 'make notary' with Developer ID)." >&2
    exit 1
  fi
fi
echo "signing with: $identity"

# Tauri signs the app and its sidecars (hardened runtime + Entitlements.plist).
(cd "$root/apps/desktop" && APPLE_SIGNING_IDENTITY="$identity" pnpm tauri build --bundles app)

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
ditto "$bundle/macos/Calls.app" "$stage/Calls.app"
ln -s /Applications "$stage/Applications"
mkdir -p "$bundle/dmg"
rm -f "$dmg"
hdiutil create -volname Calls -srcfolder "$stage" -format UDZO -quiet "$dmg"
codesign --sign "$identity" --timestamp "$dmg"

if ! $notarize; then
  cat <<EOF
ready: $dmg

Not notarized (needs Developer ID), so on the other Mac, once:
  1. open the .dmg and drag Calls to Applications
  2. open Calls: macOS blocks it; System Settings > Privacy & Security > "Open Anyway"
     (or in a terminal: xattr -dr com.apple.quarantine /Applications/Calls.app)
EOF
  exit 0
fi

echo "notarizing (usually 2-10 minutes)..."
result="$(xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait --output-format json)"
status="$(plutil -extract status raw - <<<"$result")"
if [ "$status" != "Accepted" ]; then
  echo "error: notarization $status. Apple's log:" >&2
  xcrun notarytool log "$(plutil -extract id raw - <<<"$result")" --keychain-profile "$profile" >&2
  exit 1
fi

# Staples the ticket, so it opens without asking Apple (offline, or behind a proxy).
xcrun stapler staple -q "$dmg"
spctl --assess --type open --context context:primary-signature "$dmg"
echo "ready: $dmg"
