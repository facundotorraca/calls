import pytest

from calls import cli
from calls.audio import AudioError, AudioTrack
from calls.transcription import Transcriber

from .fakes import FakeWhisperModel


@pytest.fixture
def fake_transcriber(monkeypatch, transcript):
    """Make the CLI transcribe with canned output instead of loading Whisper."""
    requests = []

    class StubTranscriber(Transcriber):
        def transcribe_file(self, path, **kwargs):
            requests.append((path, kwargs))
            return transcript

    def load(model, *, device):
        requests.append(("load", model, device))
        return StubTranscriber(FakeWhisperModel([]))

    monkeypatch.setattr(cli.Transcriber, "load", staticmethod(load))
    return requests


def test_transcribe_writes_txt_next_to_recording_by_default(tmp_path, fake_transcriber, capsys):
    recording = tmp_path / "demo.mkv"

    assert cli.main(["transcribe", str(recording)]) == 0

    assert (tmp_path / "demo.txt").read_text(encoding="utf-8").startswith("Hola")
    assert capsys.readouterr().out.strip() == str(tmp_path / "demo.txt")


def test_transcribe_options_reach_the_transcriber(tmp_path, fake_transcriber):
    recording = tmp_path / "demo.mkv"
    out = tmp_path / "out"

    cli.main(
        [
            "transcribe", str(recording),
            "-f", "srt", "-f", "json",
            "-o", str(out),
            "-m", "small", "-l", "es", "-t", "1", "--device", "cpu",
        ]
    )  # fmt: skip

    load, (path, kwargs) = fake_transcriber
    assert load == ("load", "small", "cpu")
    assert path == recording
    assert kwargs["track"] == 1
    assert kwargs["language"] == "es"
    assert sorted(p.name for p in out.iterdir()) == ["demo.json", "demo.srt"]


def test_tracks_lists_labels(monkeypatch, capsys):
    track = AudioTrack(1, "aac", 2, 48000, "Mic/Aux", None)
    monkeypatch.setattr(cli, "list_audio_tracks", lambda path: [track])

    assert cli.main(["tracks", "rec.mkv"]) == 0
    assert capsys.readouterr().out == "1: aac, 2ch, 48000 Hz, Mic/Aux\n"


def test_audio_errors_become_a_clean_exit(monkeypatch, capsys):
    def broken(path):
        raise AudioError("cannot open rec.mkv")

    monkeypatch.setattr(cli, "list_audio_tracks", broken)

    assert cli.main(["tracks", "rec.mkv"]) == 1
    assert capsys.readouterr().err == "error: cannot open rec.mkv\n"
