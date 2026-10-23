"""Audio extraction from recordings.

OBS can record several audio tracks into one file (for example, the microphone on
one track and desktop audio on another). Whisper needs a single mono stream, so
this module lets the caller list the tracks and decode the one they want.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import av
import numpy as np

SAMPLE_RATE = 16_000
"""Sample rate Whisper models expect, in Hz."""


class AudioError(Exception):
    """The recording cannot be read or has no usable audio."""


@dataclass(frozen=True)
class AudioTrack:
    """An audio stream inside a recording.

    ``index`` is the position among the file's *audio* streams (0 = first audio
    track), which is what :func:`load_audio` takes, not the container stream id.
    """

    index: int
    codec: str
    channels: int
    sample_rate: int
    title: str | None
    language: str | None

    @property
    def label(self) -> str:
        """Human-readable summary, e.g. ``"1: aac, 2ch, 48000 Hz, Mic/Aux"``."""
        details = [self.codec, f"{self.channels}ch", f"{self.sample_rate} Hz"]
        details += [value for value in (self.title, self.language) if value]
        return f"{self.index}: {', '.join(details)}"


def list_audio_tracks(path: str | Path) -> list[AudioTrack]:
    """Return the audio tracks in ``path``, in file order."""
    with _open(path) as container:
        return [
            AudioTrack(
                index=i,
                codec=stream.codec_context.name,
                channels=stream.codec_context.channels,
                sample_rate=stream.codec_context.sample_rate,
                title=stream.metadata.get("title"),
                language=stream.metadata.get("language"),
            )
            for i, stream in enumerate(container.streams.audio)
        ]


def load_audio(path: str | Path, track: int = 0) -> np.ndarray:
    """Decode one audio track as mono float32 samples in [-1, 1] at :data:`SAMPLE_RATE`."""
    with _open(path) as container:
        streams = container.streams.audio
        if not streams:
            raise AudioError(f"{path} has no audio tracks")
        if not 0 <= track < len(streams):
            raise AudioError(
                f"{path} has {len(streams)} audio track(s); track {track} does not exist"
            )

        resampler = av.AudioResampler(format="s16", layout="mono", rate=SAMPLE_RATE)
        chunks: list[np.ndarray] = []
        for frame in container.decode(streams[track]):
            chunks.extend(out.to_ndarray().ravel() for out in resampler.resample(frame))
        chunks.extend(out.to_ndarray().ravel() for out in resampler.resample(None))

    if not chunks:
        return np.zeros(0, dtype=np.float32)
    return np.concatenate(chunks).astype(np.float32) / 32768.0


def _open(path: str | Path) -> av.container.InputContainer:
    try:
        return av.open(str(path))
    except (FileNotFoundError, av.error.FFmpegError) as exc:
        raise AudioError(f"cannot open {path}: {exc}") from exc
