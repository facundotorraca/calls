"""Speech-to-text on top of faster-whisper."""

from __future__ import annotations

from collections.abc import Callable, Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

import numpy as np

from calls.audio import SAMPLE_RATE, load_audio

DEFAULT_MODEL = "turbo"
"""large-v3-turbo: close to large-v3 accuracy at a fraction of the cost."""

MODELS = ("tiny", "base", "small", "medium", "large-v3", "turbo")

ProgressCallback = Callable[[float], None]
"""Receives the fraction of audio processed so far, from 0.0 to 1.0."""


@dataclass(frozen=True)
class Segment:
    start: float
    end: float
    text: str


@dataclass(frozen=True)
class Transcript:
    segments: tuple[Segment, ...]
    language: str
    duration: float

    @property
    def text(self) -> str:
        return "\n".join(segment.text for segment in self.segments)


class WhisperModel(Protocol):
    """The part of ``faster_whisper.WhisperModel`` this module relies on."""

    def transcribe(self, audio: np.ndarray, **kwargs: Any) -> tuple[Iterable[Any], Any]: ...


class Transcriber:
    """Turns audio into a :class:`Transcript`.

    Loading a model is slow, so build one ``Transcriber`` and reuse it for
    several files.
    """

    def __init__(self, model: WhisperModel) -> None:
        self._model = model

    @classmethod
    def load(
        cls, model: str = DEFAULT_MODEL, *, device: str = "auto", compute_type: str = "default"
    ) -> Transcriber:
        """Load a Whisper model, downloading it on first use."""
        # Imported here so commands that never transcribe (listing tracks, --help)
        # don't pay for loading CTranslate2.
        from faster_whisper import WhisperModel

        return cls(WhisperModel(model, device=device, compute_type=compute_type))

    def transcribe(
        self,
        audio: np.ndarray,
        *,
        language: str | None = None,
        progress: ProgressCallback | None = None,
    ) -> Transcript:
        """Transcribe mono float32 audio at :data:`calls.audio.SAMPLE_RATE`.

        ``language`` is an ISO 639-1 code such as ``"es"``; ``None`` auto-detects it.
        """
        duration = len(audio) / SAMPLE_RATE
        raw_segments, info = self._model.transcribe(
            audio,
            language=language,
            # Recordings have long silences; skipping them avoids Whisper
            # hallucinating text over quiet stretches.
            vad_filter=True,
        )

        segments = []
        for raw in raw_segments:  # lazy: decoding happens while iterating
            segments.append(Segment(start=raw.start, end=raw.end, text=raw.text.strip()))
            if progress and duration:
                progress(min(raw.end / duration, 1.0))
        if progress:
            progress(1.0)

        return Transcript(segments=tuple(segments), language=info.language, duration=duration)

    def transcribe_file(
        self,
        path: str | Path,
        *,
        track: int = 0,
        language: str | None = None,
        progress: ProgressCallback | None = None,
    ) -> Transcript:
        """Decode audio ``track`` of ``path`` and transcribe it."""
        return self.transcribe(load_audio(path, track), language=language, progress=progress)
