"""Speaker diarization: who spoke when, with pyannote's community-1 pipeline, run locally.

Two actions (see workers/README.md for the protocol):
- {"action": "download", "token": "hf_..."}: fetches the model once. It is gated on
  Hugging Face (free; accepting its conditions is required), hence the token. The token
  is used for this download only and never stored.
- {"audio": "/path/to/16k-mono.wav", "num_speakers": null}: diarizes, offline.
  -> {"turns": [{"start_ms": 0, "end_ms": 1200, "speaker": "SPEAKER_00"}, ...]}

The model is kept under $HF_HOME, which the app points inside its own data dir.
"""

import json
import sys
import wave

MODEL = "pyannote/speaker-diarization-community-1"


def emit(**message) -> None:
    print(json.dumps(message), flush=True)


def download(token: str) -> dict:
    from huggingface_hub import snapshot_download, whoami
    from huggingface_hub.errors import GatedRepoError, HfHubHTTPError

    def refused(e: HfHubHTTPError) -> bool:
        return e.response is not None and e.response.status_code in (401, 403)

    emit(progress=0.1)
    # The token first: a wrong token and conditions not accepted both fail the download
    # with a 401, and each needs a different fix.
    try:
        whoami(token=token)
    except HfHubHTTPError as e:
        if refused(e):
            raise RuntimeError(
                "Hugging Face no reconoce el token: copialo de nuevo desde "
                "https://huggingface.co/settings/tokens"
            ) from None
        raise
    not_accepted = (
        "Falta aceptar las condiciones del modelo: entrá a "
        f"https://huggingface.co/{MODEL} con tu cuenta, completá el formulario de arriba "
        "y tocá «Agree and access repository»"
    )
    try:
        snapshot_download(MODEL, token=token)
    except GatedRepoError:
        raise RuntimeError(not_accepted) from None
    except HfHubHTTPError as e:
        if refused(e):
            raise RuntimeError(not_accepted) from None
        raise
    emit(progress=1.0)
    return {"model": MODEL}


def load_wav(path: str):
    """The app's 16 kHz mono 16-bit WAV as a (1, samples) float tensor. Read here rather
    than by pyannote, so decoding needs no ffmpeg libraries."""
    import numpy as np
    import torch

    with wave.open(path, "rb") as f:
        if f.getsampwidth() != 2 or f.getnchannels() != 1:
            raise ValueError(f"{path}: expected 16-bit mono WAV")
        rate = f.getframerate()
        samples = np.frombuffer(f.readframes(f.getnframes()), dtype=np.int16)
    waveform = torch.from_numpy(samples.astype(np.float32) / 32768.0).unsqueeze(0)
    return waveform, rate


class Progress:
    """pyannote reports each stage of the pipeline; segmentation and embeddings are what
    take time, so they make up the bar."""

    WEIGHTS = {"segmentation": 0.3, "embeddings": 0.65}

    def __init__(self):
        self.done = 0.0
        self.last = -1.0

    def __call__(self, step_name, step_artifact, file=None, total=None, completed=None):
        weight = self.WEIGHTS.get(step_name)
        if weight is None:
            return
        fraction = completed / total if total and completed is not None else 1.0
        offset = 0.0 if step_name == "segmentation" else self.WEIGHTS["segmentation"]
        value = min(0.99, offset + weight * fraction)
        if value - self.last >= 0.01:
            self.last = value
            emit(progress=round(value, 3))


def diarize(request: dict) -> dict:
    import numpy as np
    import torch
    from pyannote.audio import Pipeline

    try:
        pipeline = Pipeline.from_pretrained(MODEL)
    except Exception as e:
        raise RuntimeError("the diarization model is not downloaded") from e
    if pipeline is None:
        raise RuntimeError("the diarization model is not downloaded")

    # Apple GPU when there is one: several times faster than the CPU.
    if torch.backends.mps.is_available():
        pipeline.to(torch.device("mps"))

    waveform, rate = load_wav(request["audio"])
    options = {}
    if request.get("num_speakers"):
        options["num_speakers"] = int(request["num_speakers"])
    output = pipeline({"waveform": waveform, "sample_rate": rate}, hook=Progress(), **options)

    # The exclusive version gives every moment to one speaker, which is what a transcript
    # line needs; overlapped speech goes to whoever is most likely talking.
    annotation = getattr(output, "exclusive_speaker_diarization", output)
    turns = [
        {"start_ms": round(turn.start * 1000), "end_ms": round(turn.end * 1000), "speaker": speaker}
        for turn, _, speaker in annotation.itertracks(yield_label=True)
    ]
    # Each voice's fingerprint (an embedding), so the app can recognize it in other calls.
    voices = {}
    embeddings = getattr(output, "speaker_embeddings", None)
    if embeddings is not None:
        for label, embedding in zip(output.speaker_diarization.labels(), embeddings):
            if np.isfinite(embedding).all():
                voices[label] = [round(float(x), 5) for x in embedding]
    emit(progress=1.0)
    return {"turns": turns, "voices": voices}


def handle(request: dict) -> dict:
    if request.get("action") == "download":
        return download(request["token"])
    return diarize(request)


if __name__ == "__main__":
    try:
        emit(result=handle(json.load(sys.stdin)))
    except Exception as e:  # report any failure through the protocol
        emit(error=f"{type(e).__name__}: {e}" if not isinstance(e, RuntimeError) else str(e))
        sys.exit(1)
