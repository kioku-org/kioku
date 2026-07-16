"""Chirp 3 STT via OpenRouter — cloud backend for /v1/audio/transcriptions.

The whole integration is one JSON call: base64 WAV in, {"text": ...} out.
OpenRouter limits (probed live 2026-07-16): response_format=json only — no
timestamps and no confidence fields — and the `language` request param is
ignored (chirp auto-detects internally but the detection isn't echoed back).
The result therefore maps to ONE whole-window segment, labeled with
CHIRP_LANGUAGE_LABEL when the caller doesn't pin a language.
"""
import base64
import io
import json
import os
import urllib.error
import urllib.request
from typing import Any, Dict, List, Optional

import numpy as np
import soundfile as sf
from fastapi import HTTPException

API_KEY = os.getenv("OPENROUTER_API_KEY", "").strip()
URL = os.getenv("OPENROUTER_STT_URL", "https://openrouter.ai/api/v1/audio/transcriptions")
MODEL = os.getenv("CHIRP_MODEL", "google/chirp-3")
# Chirp's front-end discards quiet audio that whisper handles fine (Meet
# capture sits around RMS 0.01): gain to this RMS before sending. Calibrated
# on captured live-meeting windows — 0.05 still under-transcribes.
TARGET_RMS = float(os.getenv("CHIRP_TARGET_RMS", "0.1"))
# Label only — must be a whisper-valid code; meeting-api's segment validation
# rejects anything else (e.g. "unknown"), silently emptying the transcript.
LANGUAGE_LABEL = os.getenv("CHIRP_LANGUAGE_LABEL", "en").strip() or "en"


def transcribe(audio: np.ndarray, sample_rate: int, language: Optional[str] = None) -> Dict[str, Any]:
    """One OpenRouter round-trip. Returns the Whisper-shaped response dict."""
    rms = float(np.sqrt(np.mean(audio ** 2))) if audio.size else 0.0
    if 0.001 < rms < TARGET_RMS:
        # ponytail: hard clip after gain; a limiter is overkill for ASR input.
        audio = np.clip(audio * (TARGET_RMS / rms), -1.0, 1.0)

    wav = io.BytesIO()
    sf.write(wav, audio, sample_rate, format="WAV", subtype="PCM_16")
    payload = json.dumps({
        "model": MODEL,
        "input_audio": {
            "data": base64.b64encode(wav.getvalue()).decode("ascii"),
            "format": "wav",
        },
    }).encode("utf-8")
    req = urllib.request.Request(URL, data=payload, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {API_KEY}",
    })
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")[:500]
        # 429/503 stay retryable for the bot's transcription client; anything
        # else (bad key, bad payload) must fail fast, not retry-loop.
        status = e.code if e.code in (429, 503) else 502
        raise HTTPException(status_code=status, detail=f"OpenRouter {e.code}: {body}")

    text = (data.get("text") or "").strip()
    duration = len(audio) / float(sample_rate) if sample_rate else 0.0
    segments: List[Dict[str, Any]] = [{
        "id": 0, "seek": 0, "start": 0.0, "end": duration,
        "text": text, "tokens": [], "temperature": 0.0,
        "audio_start": 0.0, "audio_end": duration,
    }] if text else []
    return {
        "text": text,
        "language": language or LANGUAGE_LABEL,
        "language_probability": 0.0,
        "duration": duration,
        "segments": segments,
    }
