#!/usr/bin/env bash
# Downloads the models the app needs into the shared data dir, using the
# official scripts shipped with the pinned whisper.cpp submodule.
#   usage: scripts/download-models.sh [whisper-model] [vad-model]
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
models_dir="$HOME/Library/Application Support/calls/models"
whisper_model="${1:-large-v3-turbo}"
vad_model="${2:-silero-v6.2.0}"

git -C "$root" submodule update --init vendor/whisper.cpp
mkdir -p "$models_dir"

if [ ! -f "$models_dir/ggml-$whisper_model.bin" ]; then
  sh "$root/vendor/whisper.cpp/models/download-ggml-model.sh" "$whisper_model" "$models_dir"
fi
if [ ! -f "$models_dir/ggml-$vad_model.bin" ]; then
  sh "$root/vendor/whisper.cpp/models/download-vad-model.sh" "$vad_model" "$models_dir"
fi
echo "models ready in: $models_dir"
