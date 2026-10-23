"""Local web interface. Requires the ``ui`` extra (``pip install 'calls[ui]'``)."""

from __future__ import annotations

import tempfile
from functools import lru_cache
from pathlib import Path

import gradio as gr

from calls.audio import AudioError, list_audio_tracks
from calls.formats import OutputFormat, save
from calls.transcription import DEFAULT_MODEL, MODELS, Transcriber

AUTO_DETECT = "auto"
LANGUAGES = [
    ("Auto-detect", AUTO_DETECT),
    ("Spanish", "es"),
    ("English", "en"),
    ("Portuguese", "pt"),
    ("French", "fr"),
    ("Italian", "it"),
    ("German", "de"),
]


def build() -> gr.Blocks:
    with gr.Blocks() as app:
        gr.Markdown("# calls\nTranscribe recordings locally with Whisper.")
        with gr.Row():
            with gr.Column():
                recording = gr.File(
                    label="Recording", file_types=["video", "audio"], type="filepath"
                )
                track = gr.Dropdown(label="Audio track", choices=[], interactive=True)
                model = gr.Dropdown(choices=list(MODELS), value=DEFAULT_MODEL, label="Model")
                language = gr.Dropdown(
                    choices=LANGUAGES,
                    value=AUTO_DETECT,
                    label="Language",
                    info="Pick one or type an ISO code",
                    allow_custom_value=True,
                )
                run = gr.Button("Transcribe", variant="primary")
            with gr.Column():
                text = gr.Textbox(label="Transcript", lines=20)
                downloads = gr.File(label="Downloads", file_count="multiple")

        recording.change(_track_choices, inputs=recording, outputs=track)
        run.click(
            _transcribe, inputs=[recording, track, model, language], outputs=[text, downloads]
        )
    return app


def launch(*, port: int = 7860) -> None:
    # Bound to localhost: recordings never leave this machine.
    build().launch(server_name="127.0.0.1", server_port=port, inbrowser=True)


def _track_choices(path: str | None) -> gr.Dropdown:
    if not path:
        return gr.Dropdown(choices=[], value=None)
    try:
        tracks = list_audio_tracks(path)
    except AudioError as exc:
        raise gr.Error(str(exc)) from exc
    if not tracks:
        raise gr.Error("This file has no audio tracks.")
    return gr.Dropdown(choices=[(t.label, t.index) for t in tracks], value=tracks[0].index)


def _transcribe(
    path: str | None,
    track: int | None,
    model: str,
    language: str,
    progress: gr.Progress = gr.Progress(),  # noqa: B008 - Gradio's injection idiom
) -> tuple[str, list[str]]:
    if not path:
        raise gr.Error("Upload a recording first.")

    progress(0, desc=f"Loading model '{model}'")
    transcriber = _load_transcriber(model)
    try:
        transcript = transcriber.transcribe_file(
            path,
            track=track or 0,
            language=None if language == AUTO_DETECT else language,
            progress=lambda fraction: progress(fraction, desc="Transcribing"),
        )
    except AudioError as exc:
        raise gr.Error(str(exc)) from exc

    output_dir = Path(tempfile.mkdtemp(prefix="calls-"))
    paths = save(transcript, output_dir, Path(path).stem, [OutputFormat.TXT, OutputFormat.SRT])
    return transcript.text, [str(p) for p in paths]


@lru_cache(maxsize=1)
def _load_transcriber(model: str) -> Transcriber:
    """Keep the last model in memory; reloading takes seconds to minutes."""
    return Transcriber.load(model)
