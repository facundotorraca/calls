#!/usr/bin/env bash
# Builds the external binaries the app runs as sidecars and places them where Tauri
# expects them:
# apps/desktop/src-tauri/binaries/<name>-<target-triple>
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/vendor/whisper.cpp"
build="$src/build"
out="$root/apps/desktop/src-tauri/binaries"
triple="$(rustc --print host-tuple)"

git -C "$root" submodule update --init vendor/whisper.cpp

cmake -S "$src" -B "$build" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF \
  -DGGML_METAL_EMBED_LIBRARY=ON \
  -DWHISPER_BUILD_TESTS=OFF \
  -DWHISPER_BUILD_SERVER=OFF
cmake --build "$build" --config Release --target whisper-cli -j

mkdir -p "$out"
cp "$build/bin/whisper-cli" "$out/whisper-cli-$triple"
echo "sidecar ready: $out/whisper-cli-$triple"

# calls-audio: call detection and recording (Swift, Apple's audio APIs).
swift build -c release --package-path "$root/sidecars/calls-audio"
cp "$root/sidecars/calls-audio/.build/release/calls-audio" "$out/calls-audio-$triple"
echo "sidecar ready: $out/calls-audio-$triple"
