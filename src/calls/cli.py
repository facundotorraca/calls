"""Command-line interface: ``calls transcribe``, ``calls tracks`` and ``calls ui``."""

from __future__ import annotations

import argparse
import sys
from collections.abc import Sequence
from pathlib import Path

from calls import __version__
from calls.audio import AudioError, list_audio_tracks
from calls.formats import OutputFormat, save
from calls.transcription import DEFAULT_MODEL, MODELS, Transcriber


def main(argv: Sequence[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    try:
        return args.handler(args)
    except AudioError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="calls", description="Transcribe screen and call recordings locally with Whisper."
    )
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    commands = parser.add_subparsers(dest="command", required=True)

    transcribe = commands.add_parser(
        "transcribe",
        help="transcribe a recording",
        description="Transcribe a video or audio recording (OBS .mkv/.mp4, .wav, .mp3, ...).",
    )
    transcribe.add_argument("input", type=Path, help="recording to transcribe")
    transcribe.add_argument(
        "-f",
        "--format",
        dest="formats",
        action="append",
        choices=[fmt.value for fmt in OutputFormat],
        help="output format; repeat for several (default: txt)",
    )
    transcribe.add_argument(
        "-o",
        "--output-dir",
        type=Path,
        help="where to write the transcript (default: next to the recording)",
    )
    transcribe.add_argument(
        "-m",
        "--model",
        default=DEFAULT_MODEL,
        choices=MODELS,
        help="Whisper model (default: %(default)s)",
    )
    transcribe.add_argument(
        "-l", "--language", help="spoken language as an ISO code, e.g. 'es' (default: auto-detect)"
    )
    transcribe.add_argument(
        "-t",
        "--track",
        type=int,
        default=0,
        help="audio track to transcribe; see 'calls tracks' (default: %(default)s)",
    )
    transcribe.add_argument(
        "--device", default="auto", choices=["auto", "cpu", "cuda"], help="(default: %(default)s)"
    )
    transcribe.set_defaults(handler=_transcribe)

    tracks = commands.add_parser("tracks", help="list the audio tracks in a recording")
    tracks.add_argument("input", type=Path)
    tracks.set_defaults(handler=_tracks)

    ui = commands.add_parser("ui", help="open the web interface (needs the 'ui' extra)")
    ui.add_argument("--port", type=int, default=7860, help="(default: %(default)s)")
    ui.set_defaults(handler=_ui)

    return parser


def _transcribe(args: argparse.Namespace) -> int:
    formats = args.formats or [OutputFormat.TXT]
    output_dir = args.output_dir or args.input.parent

    print(f"Loading model '{args.model}'...", file=sys.stderr)
    transcriber = Transcriber.load(args.model, device=args.device)

    print(f"Transcribing {args.input}...", file=sys.stderr)
    transcript = transcriber.transcribe_file(
        args.input, track=args.track, language=args.language, progress=_print_progress
    )
    print(file=sys.stderr)

    for path in save(transcript, output_dir, args.input.stem, formats):
        print(path)
    return 0


def _tracks(args: argparse.Namespace) -> int:
    tracks = list_audio_tracks(args.input)
    if not tracks:
        print(f"{args.input} has no audio tracks", file=sys.stderr)
        return 1
    for track in tracks:
        print(track.label)
    return 0


def _ui(args: argparse.Namespace) -> int:
    try:
        from calls.ui import launch
    except ModuleNotFoundError as exc:
        if exc.name != "gradio":
            raise
        print("error: the web interface needs: pip install 'calls[ui]'", file=sys.stderr)
        return 1
    launch(port=args.port)
    return 0


def _print_progress(fraction: float) -> None:
    print(f"\r  {fraction:6.1%}", end="", file=sys.stderr, flush=True)


if __name__ == "__main__":
    sys.exit(main())
