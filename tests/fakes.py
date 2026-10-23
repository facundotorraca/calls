from __future__ import annotations

from types import SimpleNamespace
from typing import Any

import numpy as np


class FakeWhisperModel:
    """Stands in for faster_whisper.WhisperModel; returns canned segments."""

    def __init__(self, segments: list[tuple[float, float, str]], language: str = "es") -> None:
        self._segments = segments
        self._language = language
        self.calls: list[tuple[np.ndarray, dict[str, Any]]] = []

    def transcribe(self, audio: np.ndarray, **kwargs: Any):
        self.calls.append((audio, kwargs))
        segments = (
            SimpleNamespace(start=start, end=end, text=text) for start, end, text in self._segments
        )
        return segments, SimpleNamespace(language=self._language)
