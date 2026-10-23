"""Serialization of transcripts to text, subtitle and JSON formats."""

from __future__ import annotations

import json
from collections.abc import Callable, Iterable
from enum import StrEnum
from pathlib import Path

from calls.transcription import Transcript


class OutputFormat(StrEnum):
    TXT = "txt"
    SRT = "srt"
    VTT = "vtt"
    JSON = "json"


def render(transcript: Transcript, fmt: OutputFormat | str) -> str:
    """Serialize ``transcript``; the value of ``fmt`` doubles as the file extension."""
    return _RENDERERS[OutputFormat(fmt)](transcript)


def save(
    transcript: Transcript, directory: Path, name: str, formats: Iterable[OutputFormat | str]
) -> list[Path]:
    """Write ``<directory>/<name>.<ext>`` for each format and return the paths written."""
    directory.mkdir(parents=True, exist_ok=True)
    paths = []
    for fmt in dict.fromkeys(OutputFormat(fmt) for fmt in formats):
        # Not Path.with_suffix: names like "call.2026-10-23" would lose their tail.
        path = directory / f"{name}.{fmt.value}"
        path.write_text(render(transcript, fmt), encoding="utf-8")
        paths.append(path)
    return paths


def format_timestamp(seconds: float, *, decimal_marker: str) -> str:
    """Format as ``HH:MM:SS<marker>mmm``, the form SRT and WebVTT share."""
    # Work in whole milliseconds so 59.9996 rounds to 01:00.000, not 00:59.1000.
    millis = round(seconds * 1000)
    hours, millis = divmod(millis, 3_600_000)
    minutes, millis = divmod(millis, 60_000)
    secs, millis = divmod(millis, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d}{decimal_marker}{millis:03d}"


def _render_txt(transcript: Transcript) -> str:
    return transcript.text + "\n"


def _render_srt(transcript: Transcript) -> str:
    blocks = (
        f"{number}\n"
        f"{format_timestamp(seg.start, decimal_marker=',')} --> "
        f"{format_timestamp(seg.end, decimal_marker=',')}\n"
        f"{seg.text}\n"
        for number, seg in enumerate(transcript.segments, start=1)
    )
    return "\n".join(blocks)


def _render_vtt(transcript: Transcript) -> str:
    cues = (
        f"{format_timestamp(seg.start, decimal_marker='.')} --> "
        f"{format_timestamp(seg.end, decimal_marker='.')}\n"
        f"{seg.text}\n"
        for seg in transcript.segments
    )
    return "WEBVTT\n\n" + "\n".join(cues)


def _render_json(transcript: Transcript) -> str:
    payload = {
        "language": transcript.language,
        "duration": transcript.duration,
        "segments": [
            {"start": seg.start, "end": seg.end, "text": seg.text} for seg in transcript.segments
        ],
    }
    return json.dumps(payload, ensure_ascii=False, indent=2) + "\n"


_RENDERERS: dict[OutputFormat, Callable[[Transcript], str]] = {
    OutputFormat.TXT: _render_txt,
    OutputFormat.SRT: _render_srt,
    OutputFormat.VTT: _render_vtt,
    OutputFormat.JSON: _render_json,
}
