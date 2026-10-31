#!/usr/bin/env bash
# Builds ftor-calls.app and installs it in /Applications, replacing the old one.
# Settings, models and calls live in ~/Library/Application Support/calls, so
# they survive. To update: git pull && make install
#   usage: scripts/install.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="/Applications/ftor-calls.app"
# Its name before it was ftor-calls; removed so there are not two.
old="/Applications/Calls.app"
built="$root/target/release/bundle/macos/ftor-calls.app"

# Signed with the Developer ID certificate, like the releases: macOS keys the permissions it
# gave the app to that signature, so a source build and a release share them.
identity="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' | head -1)"
if [ -n "$identity" ]; then
  export APPLE_SIGNING_IDENTITY="$identity"
else
  echo "warning: no Developer ID certificate, signing ad-hoc: macOS will ask for permissions again." >&2
  echo "         create it in Xcode > Settings > Accounts > Manage Certificates." >&2
fi

(cd "$root/apps/desktop" && pnpm tauri build --bundles app)

# Quitting normally lets an ongoing recording finalize its files.
if pgrep -xq calls-desktop; then
  echo "quitting ftor-calls..."
  osascript -e 'quit app id "dev.facundotorraca.calls"'
  while pgrep -xq calls-desktop; do sleep 0.5; done
fi

rm -rf "$app" "$old"
ditto "$built" "$app"
echo "installed $app ($(git -C "$root" describe --always --dirty))"
open "$app"
