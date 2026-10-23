from pathlib import Path

import av
import numpy as np
import pytest

from calls.audio import SAMPLE_RATE, AudioError, list_audio_tracks, load_audio

SOURCE_RATE = 48_000
DURATION = 1.5


def _write_recording(path: Path, tracks: list[np.ndarray]) -> Path:
    """Write a Matroska file (OBS's default container) with one mono PCM stream per track."""
    with av.open(str(path), mode="w", format="matroska") as container:
        streams = [container.add_stream("pcm_s16le", rate=SOURCE_RATE) for _ in tracks]
        for stream in streams:
            stream.layout = "mono"
        for stream, samples in zip(streams, tracks, strict=True):
            pcm = (samples * 32767).astype(np.int16).reshape(1, -1)
            frame = av.AudioFrame.from_ndarray(pcm, format="s16", layout="mono")
            frame.sample_rate = SOURCE_RATE
            frame.pts = 0
            for packet in stream.encode(frame):
                container.mux(packet)
        for stream in streams:
            for packet in stream.encode(None):
                container.mux(packet)
    return path


@pytest.fixture
def two_track_recording(tmp_path) -> Path:
    t = np.arange(int(SOURCE_RATE * DURATION)) / SOURCE_RATE
    tone = 0.5 * np.sin(2 * np.pi * 440 * t)
    silence = np.zeros_like(t)
    return _write_recording(tmp_path / "recording.mkv", [silence, tone])


def test_lists_every_audio_track(two_track_recording):
    tracks = list_audio_tracks(two_track_recording)

    assert [track.index for track in tracks] == [0, 1]
    assert all(track.channels == 1 and track.sample_rate == SOURCE_RATE for track in tracks)
    assert tracks[1].label.startswith("1: pcm_s16le, 1ch, 48000 Hz")


def test_loads_selected_track_as_16khz_mono(two_track_recording):
    silence = load_audio(two_track_recording, track=0)
    tone = load_audio(two_track_recording, track=1)

    for samples in (silence, tone):
        assert samples.dtype == np.float32
        assert samples.ndim == 1
        assert len(samples) == pytest.approx(DURATION * SAMPLE_RATE, rel=0.02)
    assert np.abs(silence).max() < 0.01
    assert 0.4 < np.abs(tone).max() <= 1.0


def test_rejects_missing_track(two_track_recording):
    with pytest.raises(AudioError, match="2 audio track"):
        load_audio(two_track_recording, track=2)


def test_rejects_unreadable_file(tmp_path):
    with pytest.raises(AudioError, match="cannot open"):
        list_audio_tracks(tmp_path / "missing.mkv")
