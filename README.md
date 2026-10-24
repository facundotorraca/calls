# calls

Local transcription of calls. When a call starts (Zoom, Meet, Teams...) an island shows up at
the top of the screen to record it; when it ends, you get a transcript with who said what.
You can also drop any existing recording on the app. Everything runs on your machine.

## Setup

Requires macOS 15+ on Apple Silicon, Xcode (for Swift), Rust, pnpm, `brew install cmake ffmpeg`.

```sh
make setup   # JS deps + builds the sidecars + downloads models (~1.6 GB)
make dev     # runs the app
```

`make build` produces `Calls.app` / `.dmg` in `target/release/bundle/`.
`cargo run -p calls-cli -- doctor` checks what is installed.

## Architecture

```
apps/desktop/          Tauri app
  src/                 React UI (TypeScript)
  src-tauri/           thin Rust layer: Tauri commands -> core
crates/
  core/                domain, storage, pipeline. No UI, no concrete engines
  engines/             adapters running external tools (ffmpeg, whisper-cli)
  cli/                 `calls` command, same pipeline as the app
sidecars/calls-audio/  Swift: call detection and recording with Apple's audio APIs
vendor/whisper.cpp/    pinned submodule, built into the whisper-cli sidecar
scripts/               build sidecars, download models
```

Dependencies point inwards: `desktop`/`cli` -> `engines` -> `core`. The core never knows
which tool does the work, only the traits it needs (`AudioExtractor`, `Transcriber`).

**AI engines run as separate processes.** We call official binaries (whisper.cpp's
`whisper-cli`, `ffmpeg`) through their CLI instead of linking third-party bindings. Swapping
an engine means writing another adapter in `engines`. Future engines follow the same rule:
LLMs over HTTP, Python-only models as a sidecar worker.

### Recording

`calls-audio` reports every second which processes use the mic or play audio (CoreAudio).
`CallDetector` (`crates/core/src/detect.rs`) turns that into "call started / ended": a call app
(`call_apps` in settings) holding the mic for 2 s starts a call; the app not using audio at all
for 10 s ends it. Muting yourself does not end the call, since the other side keeps playing.

Recording writes two tracks into the call folder:

- `mic.m4a`: your microphone, speaker "Yo".
- `system.m4a`: only the call app's audio (a CoreAudio process tap), speaker "Ellos".

Without headphones the other side leaks into your mic; the transcribe step drops mic segments
that repeat the other side at the same time. If the app quits mid-recording, the sidecar
finalizes the files and the call is processed on next launch.

Permissions (asked the first time you record, by `Calls.app`): Microphone, and Screen & System
Audio Recording (the "system audio only" kind). Dev builds run under your terminal, so macOS
asks for it instead; test recording with `make build`. Ad-hoc signed builds may ask again
after rebuilding.

### Pipeline

Each call goes through a list of steps (`crates/core/src/steps.rs`):

```
extract_audio -> transcribe -> render_markdown
```

Completed steps are recorded in `call.json`, so processing is resumable (the app resumes
pending calls on start) and new steps run over old calls automatically.

To add a feature (summary, action items, export to Notion...):

1. Implement `Step` (`id()` + `run()`), reading/writing files in the call folder.
2. Add it to `default_pipeline` in `crates/engines/src/lib.rs`.
3. Optionally add a label in `STEP_LABELS` (`apps/desktop/src/api.ts`).

### Storage

Plain files are the source of truth, one folder per call in `~/Calls`:

```
~/Calls/2026-10-23_1530_acme-kickoff/
  call.json          metadata and processing state
  audio.wav          16 kHz mono audio extracted from the recording
  transcript.json    segments with timestamps (and speaker, once available)
  transcript.md      readable transcript
```

The original recording is referenced, never copied or moved. Settings and models live in
`~/Library/Application Support/calls/` (`settings.json`, `models/`).

### Settings

`~/Library/Application Support/calls/settings.json`, every field optional:

```json
{
  "library_dir": "/Users/me/Calls",
  "language": "auto",
  "whisper_model": "large-v3-turbo",
  "vad_model": "silero-v6.2.0",
  "detect_calls": true,
  "auto_record": false,
  "call_apps": [{ "bundle_prefix": "us.zoom.xos", "name": "Zoom" }]
}
```

## Roadmap

- Transcript as paragraphs per speaker.
- Speaker diarization of the other side (pyannote community-1, Python sidecar worker).
- Local LLM (llama.cpp `llama-server`): speaker names, summary, action items.
- Search across calls.
- Integrations (calendar for titles, Notion export).
