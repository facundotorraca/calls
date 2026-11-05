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

# ffmpeg and ffprobe, from the official release: static, LGPL and with only what the app
# uses, so they weigh a few MB. What it can import is MEDIA_EXTENSIONS in src/api.ts; the
# video in a recording is never decoded, only skipped.
ffmpeg_version=9.0.2
ffmpeg_sha256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
ffmpeg_dir="$root/target/sidecars/ffmpeg-$ffmpeg_version"
if [ ! -x "$ffmpeg_dir/ffmpeg" ] || [ ! -x "$ffmpeg_dir/ffprobe" ]; then
  mkdir -p "$ffmpeg_dir"
  curl -fsSL "https://ffmpeg.org/releases/ffmpeg-$ffmpeg_version.tar.xz" -o "$ffmpeg_dir/source.tar.xz"
  echo "$ffmpeg_sha256  $ffmpeg_dir/source.tar.xz" | shasum -a 256 -c -
  tar -xf "$ffmpeg_dir/source.tar.xz" -C "$ffmpeg_dir" --strip-components 1
  (
    cd "$ffmpeg_dir"
    ./configure \
      --disable-everything --disable-autodetect --disable-network --disable-doc --disable-debug \
      --disable-avdevice --disable-swscale --disable-ffplay \
      --enable-protocol=file,pipe \
      --enable-demuxer=mov,matroska,mp3,aac,wav,flac,ogg \
      --enable-decoder=aac,alac,mp3,mp3float,opus,vorbis,flac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le \
      --enable-parser=aac,flac,mpegaudio,opus,vorbis,h264,hevc,av1,vp8,vp9 \
      --enable-encoder=aac,pcm_s16le \
      --enable-muxer=wav,ipod \
      --enable-filter=adelay,amix,alimiter,aresample,aformat,anull
    make -j"$(sysctl -n hw.ncpu)"
  )
fi
cp "$ffmpeg_dir/ffmpeg" "$out/ffmpeg-$triple"
cp "$ffmpeg_dir/ffprobe" "$out/ffprobe-$triple"
echo "sidecars ready: $out/ffmpeg-$triple, $out/ffprobe-$triple"

# uv: runs the Python workers (voices), and installs Python itself. The official binary.
uv_version=0.13.7
uv_sha256=81b811b57e4624df7ed3aac148e0359100d9962f3d9dce00aec533dc2e086731
uv_dir="$root/target/sidecars/uv-$uv_version"
if [ ! -x "$uv_dir/uv" ]; then
  mkdir -p "$uv_dir"
  curl -fsSL "https://github.com/astral-sh/uv/releases/download/$uv_version/uv-aarch64-apple-darwin.tar.gz" \
    -o "$uv_dir/source.tar.gz"
  echo "$uv_sha256  $uv_dir/source.tar.gz" | shasum -a 256 -c -
  tar -xf "$uv_dir/source.tar.gz" -C "$uv_dir" --strip-components 1
fi
cp "$uv_dir/uv" "$out/uv-$triple"
echo "sidecar ready: $out/uv-$triple"
