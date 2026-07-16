"""Unit tests for the chirp (OpenRouter) backend — mocked HTTP, no network."""
import json
import os
import sys
from unittest.mock import patch

import numpy as np
import pytest
from fastapi import HTTPException

# Add service root to path
SERVICE_ROOT = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, SERVICE_ROOT)

import main


class _FakeResp:
    def __init__(self, payload):
        self._body = json.dumps(payload).encode()

    def read(self):
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


def test_chirp_returns_whisper_shape():
    audio = np.zeros(16000, dtype=np.float32)  # 1s
    reply = {"text": " hello world ", "usage": {"seconds": 1, "cost": 0.0003}}
    with patch.object(main.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        out = main._transcribe_chirp(audio, 16000, "en", "prior context")
    assert out["text"] == "hello world"
    assert out["language"] == "en"
    assert out["duration"] == 1.0
    assert out["segments"][0]["start"] == 0.0
    assert out["segments"][0]["end"] == 1.0
    # request went to the transcriptions endpoint carrying multipart form fields
    req = mock_open.call_args[0][0]
    assert req.full_url.endswith("/audio/transcriptions")
    assert req.get_header("Content-type", "").startswith("multipart/form-data")
    assert main.CHIRP_MODEL.encode() in req.data
    assert b'name="response_format"\r\n\r\njson' in req.data
    assert b'name="language"\r\n\r\nen' in req.data
    assert b'name="prompt"\r\n\r\nprior context' in req.data
    assert b"RIFF" in req.data  # the WAV made it into the body


def test_chirp_normalizes_quiet_audio():
    import io as _io
    import soundfile as _sf

    t = np.arange(16000, dtype=np.float32) / 16000.0
    audio = (0.01 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)  # rms ~0.007
    reply = {"text": "ok", "usage": {"seconds": 1, "cost": 0.0}}
    with patch.object(main.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        main._transcribe_chirp(audio, 16000, None, None)
    body = mock_open.call_args[0][0].data
    wav = body[body.index(b"RIFF"):]
    sent, _ = _sf.read(_io.BytesIO(wav), dtype="float32")
    rms = float(np.sqrt((sent ** 2).mean()))
    assert 0.08 < rms < 0.12  # boosted to CHIRP_TARGET_RMS, not left at 0.007

    loud = (0.5 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)  # already loud
    with patch.object(main.urllib.request, "urlopen", return_value=_FakeResp(reply)) as mock_open:
        main._transcribe_chirp(loud, 16000, None, None)
    body = mock_open.call_args[0][0].data
    sent, _ = _sf.read(_io.BytesIO(body[body.index(b"RIFF"):]), dtype="float32")
    assert float(np.abs(sent).max()) > 0.4  # untouched


def test_chirp_empty_reply_is_silence():
    audio = np.zeros(1600, dtype=np.float32)
    reply = {"text": "", "usage": {"seconds": 0, "cost": 0.0}}
    with patch.object(main.urllib.request, "urlopen", return_value=_FakeResp(reply)):
        out = main._transcribe_chirp(audio, 16000, None, None)
    assert out["text"] == ""
    # never "unknown" — meeting-api segment validation rejects it
    assert out["language"] == "en"
    assert out["segments"] == []


def test_chirp_http_error_maps_status():
    import urllib.error

    audio = np.zeros(1600, dtype=np.float32)

    def _raise(code):
        import io as _io
        return urllib.error.HTTPError("url", code, "err", {}, _io.BytesIO(b"boom"))

    # retryable passes through
    with patch.object(main.urllib.request, "urlopen", side_effect=_raise(429)):
        with pytest.raises(HTTPException) as exc:
            main._transcribe_chirp(audio, 16000, None, None)
    assert exc.value.status_code == 429

    # auth failure must NOT look transient (502, not 500/503)
    with patch.object(main.urllib.request, "urlopen", side_effect=_raise(401)):
        with pytest.raises(HTTPException) as exc:
            main._transcribe_chirp(audio, 16000, None, None)
    assert exc.value.status_code == 502
