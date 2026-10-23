from __future__ import annotations

import pytest

from calls.transcription import Segment, Transcript


@pytest.fixture
def transcript() -> Transcript:
    return Transcript(
        segments=(
            Segment(start=0.0, end=2.5, text="Hola, ¿cómo están?"),
            Segment(start=2.5, end=3661.042, text="Arrancamos con la demo."),
        ),
        language="es",
        duration=3662.0,
    )
