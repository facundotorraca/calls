# Python workers

Sidecars for work that only exists (or is best done) in Python: diarization, new models...
The core stays in Rust; each worker is a separate process with its own environment.

Each folder is a [uv](https://docs.astral.sh/uv/) project with a `main.py`. The app runs it as
`uv run --frozen --project workers/<name> python workers/<name>/main.py`, with the virtualenv
kept in `~/Library/Application Support/calls/workers/<name>` (created on first run).

## Protocol

- **stdin**: one JSON request, then EOF.
- **stdout**: one JSON object per line:
  - `{"progress": 0.4}` (optional, `0.0..=1.0`)
  - `{"result": ...}` exactly once on success
  - `{"error": "message"}` on failure (and a non-zero exit)
- **stderr**: free form logs, shown in the error if the worker fails.

## New worker

1. `cp -r workers/template workers/<name>` and rename the project in `pyproject.toml`.
2. Write `handle()` in `main.py`; add dependencies with `uv add --project workers/<name> <pkg>`.
3. Call it from Rust with `PythonWorker::locate("<name>")?.run(&request, &progress)`.
