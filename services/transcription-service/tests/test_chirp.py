"""Unit tests for the chirp (Cloud Speech v2) backend — faked client, no network."""
import io
import os
import sys
from datetime import timedelta
from unittest.mock import patch

import numpy as np
import pytest
import soundfile as sf
from fastapi import HTTPException

# Add service root to path
SERVICE_ROOT = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, SERVICE_ROOT)

import main
from google.cloud.speech_v2.types import cloud_speech
from google.api_core import exceptions as gexc


class _FakeClient:
    def __init__(self, response=None, error=None):
        self.response = response
        self.error = error
        self.last_request = None

    def recognize(self, request, timeout=None):
        self.last_request = request
        if self.error:
            raise self.error
        return self.response


def _response(*results):
    return cloud_speech.RecognizeResponse(results=list(results))


def _result(text, words=(), language="en-US"):
    return cloud_speech.SpeechRecognitionResult(
        alternatives=[cloud_speech.SpeechRecognitionAlternative(
            transcript=text,
            words=[
                cloud_speech.WordInfo(
                    word=w, start_offset=timedelta(seconds=s), end_offset=timedelta(seconds=e)
                )
                for w, s, e in words
            ],
        )],
        language_code=language,
    )


def test_chirp_returns_whisper_shape_with_timestamps():
    audio = np.zeros(16000 * 4, dtype=np.float32)  # 4s
    fake = _FakeClient(_response(
        _result("hello world", words=[("hello", 0.5, 1.0), ("world", 1.1, 1.6)]),
        _result("second bit", words=[("second", 2.0, 2.4), ("bit", 2.5, 2.8)]),
    ))
    with patch.object(main, "chirp_client", fake):
        out = main._transcribe_chirp(audio, 16000, None, None)
    assert out["text"] == "hello world second bit"
    assert out["language"] == "en"
    assert out["duration"] == 4.0
    assert len(out["segments"]) == 2
    assert out["segments"][0]["start"] == 0.5
    assert out["segments"][0]["end"] == 1.6
    assert out["segments"][1]["start"] == 2.0
    assert out["segments"][0]["words"][0]["word"] == "hello"
    # request carried the model, recognizer path, and word offsets feature
    req = fake.last_request
    assert req.config.model == main.CHIRP_MODEL
    assert main.GOOGLE_CLOUD_PROJECT in req.recognizer or "/recognizers/_" in req.recognizer
    assert req.config.features.enable_word_time_offsets
    assert list(req.config.language_codes) == main.CHIRP_LANGUAGES


def test_chirp_language_pin_overrides_default():
    audio = np.zeros(1600, dtype=np.float32)
    fake = _FakeClient(_response())
    with patch.object(main, "chirp_client", fake):
        out = main._transcribe_chirp(audio, 16000, "id-ID", None)
    assert list(fake.last_request.config.language_codes) == ["id-ID"]
    assert out["text"] == ""
    assert out["segments"] == []


def test_chirp_normalizes_quiet_audio():
    t = np.arange(16000, dtype=np.float32) / 16000.0
    quiet = (0.01 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)  # rms ~0.007
    fake = _FakeClient(_response())
    with patch.object(main, "chirp_client", fake):
        main._transcribe_chirp(quiet, 16000, None, None)
    sent, _ = sf.read(io.BytesIO(fake.last_request.content), dtype="float32")
    rms = float(np.sqrt((sent ** 2).mean()))
    assert 0.08 < rms < 0.12  # boosted to CHIRP_TARGET_RMS

    loud = (0.5 * np.sin(2 * np.pi * 220 * t)).astype(np.float32)
    with patch.object(main, "chirp_client", fake):
        main._transcribe_chirp(loud, 16000, None, None)
    sent, _ = sf.read(io.BytesIO(fake.last_request.content), dtype="float32")
    assert float(np.abs(sent).max()) > 0.4  # untouched


def test_chirp_api_error_maps_status():
    audio = np.zeros(1600, dtype=np.float32)

    with patch.object(main, "chirp_client", _FakeClient(error=gexc.ResourceExhausted("quota"))):
        with pytest.raises(HTTPException) as exc:
            main._transcribe_chirp(audio, 16000, None, None)
    assert exc.value.status_code == 429

    # auth/config failure must NOT look transient (502, not 500/503)
    with patch.object(main, "chirp_client", _FakeClient(error=gexc.PermissionDenied("no"))):
        with pytest.raises(HTTPException) as exc:
            main._transcribe_chirp(audio, 16000, None, None)
    assert exc.value.status_code == 502
