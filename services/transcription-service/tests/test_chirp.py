"""Unit tests for the chirp (OpenRouter) backend — mocked HTTP, no network."""
import base64
import io
import json
import os
import sys
from unittest.mock import patch

import numpy as np
import pytest
import soundfile as sf
from fastapi import HTTPException

# Add service root to path
SERVICE_ROOT = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, SERVICE_ROOT)

import chirp


class _FakeResp:
    def __init__(self, payload):
        self._body = json.dumps(payload).encode()

    def read(self):
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


def _sent_audio(mock_open) -> np.ndarray:
    """Decode the FLAC that transcribe() base64'd into the request body."""
    req = mock_open.call_args[0][0]
    body = json.loads(req.data)
    assert body["input_audio"]["format"] == "flac"
    flac = base64.b64decode(body["input_audio"]["data"])
    audio, _ = sf.read(io.BytesIO(flac), dtype="float32")
    return audio


def test_returns_whisper_shape():
    audio = np.zeros(16000, dtype=np.float32)  # 1s
    reply = {"text": " hello world ", "usage": {"seconds": 1, "cost": 0.0003}}
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        out = chirp.transcribe(audio, 16000, "en")
    assert out["text"] == "hello world"
    assert out["language"] == "en"
    assert out["duration"] == 1.0
    assert out["segments"][0]["start"] == 0.0
    assert out["segments"][0]["end"] == 1.0
    req = mock_open.call_args[0][0]
    assert json.loads(req.data)["model"] == chirp.MODEL


def test_sentences_get_prorated_timestamps():
    audio = np.zeros(16000 * 10, dtype=np.float32)  # 10s
    reply = {"text": "First sentence here. Second one! And a third?", "usage": {}}
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)):
        out = chirp.transcribe(audio, 16000)
    segs = out["segments"]
    assert [s["text"] for s in segs] == ["First sentence here.", "Second one!", "And a third?"]
    assert segs[0]["start"] == 0.0
    assert segs[-1]["end"] == 10.0
    # monotonic, contiguous boundaries
    for a, b in zip(segs, segs[1:]):
        assert a["end"] == b["start"]
        assert a["start"] < a["end"]
    assert out["text"] == reply["text"].strip()


def test_long_unpunctuated_text_is_chunked():
    audio = np.zeros(16000 * 20, dtype=np.float32)
    reply = {"text": " ".join(str(n) for n in range(1, 31)), "usage": {}}  # 30 words, no punctuation
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)):
        out = chirp.transcribe(audio, 16000)
    segs = out["segments"]
    assert len(segs) == 3  # 12 + 12 + 6 words
    assert segs[0]["text"].split()[-1] == "12"
    assert segs[-1]["end"] == 20.0


def test_empty_reply_is_silence_and_label_defaults():
    audio = np.zeros(1600, dtype=np.float32)
    reply = {"text": "", "usage": {"seconds": 0, "cost": 0.0}}
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)):
        out = chirp.transcribe(audio, 16000, None)
    assert out["text"] == ""
    assert out["segments"] == []
    # never "unknown" — meeting-api segment validation rejects it
    assert out["language"] == chirp.LANGUAGE_LABEL == "en"


def test_normalizes_quiet_audio():
    t = np.arange(16000, dtype=np.float32) / 16000.0
    quiet = (0.01 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)  # rms ~0.007
    reply = {"text": "ok", "usage": {}}
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        chirp.transcribe(quiet, 16000)
    rms = float(np.sqrt((_sent_audio(mock_open) ** 2).mean()))
    assert 0.08 < rms < 0.12  # boosted to TARGET_RMS

    loud = (0.5 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)
    with patch.object(chirp.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        chirp.transcribe(loud, 16000)
    assert float(np.abs(_sent_audio(mock_open)).max()) > 0.4  # untouched


def test_http_error_maps_status():
    import urllib.error

    audio = np.zeros(1600, dtype=np.float32)

    def _raise(code):
        return urllib.error.HTTPError("url", code, "err", {}, io.BytesIO(b"boom"))

    # retryable passes through
    with patch.object(chirp.urllib.request, "urlopen", side_effect=_raise(429)):
        with pytest.raises(HTTPException) as exc:
            chirp.transcribe(audio, 16000)
    assert exc.value.status_code == 429

    # auth failure must NOT look transient (502, not 500/503)
    with patch.object(chirp.urllib.request, "urlopen", side_effect=_raise(401)):
        with pytest.raises(HTTPException) as exc:
            chirp.transcribe(audio, 16000)
    assert exc.value.status_code == 502
