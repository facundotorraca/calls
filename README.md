# calls

Local transcription of calls. When a call starts (Zoom, Meet, Teams...) an island shows up at
the top of the screen to record it; when it ends, you get a transcript with who said what.
You can also drop any existing recording on the app. Everything runs on your machine.

## Setup

Requires macOS 15+ on Apple Silicon, Xcode (for Swift), Rust, pnpm, `brew install cmake ffmpeg uv`,
and [Ollama](https://ollama.com) with a model (`ollama pull qwen3.5:35b`) for summaries.

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
  engines/             adapters running external tools (ffmpeg, whisper-cli, Ollama, Python)
  cli/                 `calls` command, same pipeline as the app
sidecars/calls-audio/  Swift: call detection and recording with Apple's audio APIs
workers/               Python sidecars (see workers/README.md)
vendor/whisper.cpp/    pinned submodule, built into the whisper-cli sidecar
scripts/               build sidecars, download models
```

Dependencies point inwards: `desktop`/`cli` -> `engines` -> `core`. The core never knows
which tool does the work, only the traits it needs (`AudioExtractor`, `Transcriber`, `Llm`).

**AI engines run as separate processes.** We call official binaries (whisper.cpp's
`whisper-cli`, `ffmpeg`) through their CLI, LLMs over HTTP (Ollama) and Python-only models as
workers (`PythonWorker`, JSON over stdio, environments managed by uv), instead of linking
third-party bindings. Swapping an engine means writing another adapter in `engines`.

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
extract_audio -> transcribe -> render_markdown -> summarize
```

`summarize` asks the local LLM for a summary, decisions and action items (`summary.json`,
`summary.md`), written in `summary_language`. It runs last, so the transcript is usable even
if Ollama is closed; "Generar resumen" in the app runs it later.

Completed steps are recorded in `call.json`, so processing is resumable (the app resumes
pending calls on start) and new steps run over old calls automatically.

To add a feature (summary, action items, export to Notion...):

1. Implement `Step` (`id()` + `run()`), reading/writing files in the call folder.
2. Add it to `default_pipeline` in `crates/engines/src/lib.rs`.
3. Optionally add a label in `STEP_LABELS` (`apps/desktop/src/api.ts`).

### Storage

Plain files are the source of truth, one folder per call in `~/Calls`:

```
~/Calls/
  library.json         folder list
  2026-10-23_1530_acme-kickoff/
    call.json          metadata (title, folder) and processing state
    audio.wav          16 kHz mono audio extracted from the recording
    transcript.json    segments with timestamps (and speaker, once available)
    transcript.md      readable transcript
    summary.json/.md   summary, decisions, action items
```

Folders are labels (`folder` in `call.json`), not directories: call folders never move, so
paths stay valid while you organize. The id (folder name) never changes either; renaming a
call changes its `title` and rewrites the `.md` documents.

The original recording is referenced, never copied or moved. Settings and models live in
`~/Library/Application Support/calls/` (`settings.json`, `models/`, `workers/` environments).

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
  "call_apps": [{ "bundle_prefix": "us.zoom.xos", "name": "Zoom" }],
  "summarize": true,
  "summary_language": "español",
  "llm": { "base_url": "http://localhost:11434", "model": "qwen3.5:35b" }
}
```

## Roadmap

- Transcript as paragraphs per speaker.
- Speaker diarization (pyannote community-1 as a Python worker), word level timestamps.
- Speaker names with the LLM.
- Search across calls.
- Integrations (calendar for titles, Notion export).
