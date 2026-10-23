"""Local transcription of screen and call recordings."""

from calls.audio import AudioError, AudioTrack, list_audio_tracks, load_audio
from calls.formats import OutputFormat, render, save
from calls.transcription import Segment, Transcriber, Transcript

__version__ = "0.1.0"

__all__ = [
    "AudioError",
    "AudioTrack",
    "OutputFormat",
    "Segment",
    "Transcriber",
    "Transcript",
    "list_audio_tracks",
    "load_audio",
    "render",
    "save",
]
