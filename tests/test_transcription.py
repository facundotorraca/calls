import numpy as np
import pytest

from calls import transcription
from calls.audio import SAMPLE_RATE
from calls.transcription import Segment, Transcriber

from .fakes import FakeWhisperModel

TEN_SECONDS = np.zeros(10 * SAMPLE_RATE, dtype=np.float32)


def test_builds_transcript_from_model_segments():
    model = FakeWhisperModel([(0.0, 4.0, "  Hola "), (4.0, 10.0, "chau")], language="es")

    transcript = Transcriber(model).transcribe(TEN_SECONDS)

    assert transcript.segments == (Segment(0.0, 4.0, "Hola"), Segment(4.0, 10.0, "chau"))
    assert transcript.language == "es"
    assert transcript.duration == 10.0
    assert transcript.text == "Hola\nchau"


@pytest.mark.parametrize("language", [None, "es"])
def test_passes_language_and_enables_vad(language):
    model = FakeWhisperModel([])

    Transcriber(model).transcribe(TEN_SECONDS, language=language)

    ((_, kwargs),) = model.calls
    assert kwargs["language"] == language
    assert kwargs["vad_filter"] is True


def test_reports_progress_up_to_completion():
    model = FakeWhisperModel([(0.0, 2.5, "a"), (2.5, 5.0, "b")])
    reported = []

    Transcriber(model).transcribe(TEN_SECONDS, progress=reported.append)

    assert reported == [0.25, 0.5, 1.0]


def test_empty_audio_does_not_divide_by_zero():
    reported = []

    transcript = Transcriber(FakeWhisperModel([])).transcribe(
        np.zeros(0, dtype=np.float32), progress=reported.append
    )

    assert transcript.segments == ()
    assert reported == [1.0]


def test_transcribe_file_decodes_requested_track(monkeypatch):
    loaded = []

    def fake_load_audio(path, track):
        loaded.append((path, track))
        return TEN_SECONDS

    monkeypatch.setattr(transcription, "load_audio", fake_load_audio)
    model = FakeWhisperModel([(0.0, 1.0, "hola")])

    transcript = Transcriber(model).transcribe_file("rec.mkv", track=1, language="es")

    assert loaded == [("rec.mkv", 1)]
    assert transcript.text == "hola"
