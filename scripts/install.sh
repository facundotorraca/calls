#!/usr/bin/env bash
# Builds ftor-calls.app and installs it in /Applications, replacing the old one.
# Settings, models and calls live in ~/Library/Application Support/calls, so
# they survive. To update: git pull && make install
#   usage: scripts/install.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
identity="Calls Local Signing"
app="/Applications/ftor-calls.app"
# Its name before it was ftor-calls; removed so there are not two.
old="/Applications/Calls.app"
built="$root/target/release/bundle/macos/ftor-calls.app"

if security find-identity -v -p codesigning | grep -q "\"$identity\""; then
  export APPLE_SIGNING_IDENTITY="$identity"
else
  echo "warning: no signing identity, macOS will ask for permissions again." >&2
  echo "         run 'make signing' once to keep them across updates." >&2
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
