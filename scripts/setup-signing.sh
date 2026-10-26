#!/usr/bin/env bash
# Creates a self-signed code signing identity in the login keychain (once).
# Signing every build with the same identity keeps macOS permissions
# (microphone, system audio) across updates; ad-hoc builds lose them.
# Trusting the certificate asks for your password once.
#   usage: scripts/setup-signing.sh
set -euo pipefail

name="Calls Local Signing"
keychain="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "\"$name\""; then
  echo "signing identity ready: $name"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if security find-certificate -c "$name" -p "$keychain" >"$tmp/cert.pem" 2>/dev/null; then
  security add-trusted-cert -r trustRoot -p codeSign -k "$keychain" "$tmp/cert.pem"
  echo "trusted signing identity: $name"
  exit 0
fi

pass="$(uuidgen)"

/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 7300 \
  -subj "/CN=$name" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
  -name "$name" -passout "pass:$pass" -out "$tmp/identity.p12"
security import "$tmp/identity.p12" -k "$keychain" -P "$pass" -T /usr/bin/codesign >/dev/null
security add-trusted-cert -r trustRoot -p codeSign -k "$keychain" "$tmp/cert.pem"

echo "created signing identity: $name"
