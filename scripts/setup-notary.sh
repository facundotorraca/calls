#!/usr/bin/env bash
# Saves the notarization credentials in the login keychain (once), as the
# notarytool profile `make release` uses. Needs a "Developer ID Application"
# certificate (Xcode > Settings > Accounts > Manage Certificates > +) and an
# app-specific password (appleid.apple.com > Sign-In and Security).
#   usage: scripts/setup-notary.sh
set -euo pipefail

profile="calls-notary"

if ! security find-identity -v -p codesigning | grep -q '"Developer ID Application:'; then
  echo "error: no \"Developer ID Application\" certificate in the keychain." >&2
  echo "       create it in Xcode > Settings > Accounts > Manage Certificates > +" >&2
  exit 1
fi

# Asks for the Apple ID, team ID and app-specific password, and checks them with Apple.
xcrun notarytool store-credentials "$profile"
