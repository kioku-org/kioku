"""Cloud STT via OpenRouter — backend for /v1/audio/transcriptions.

One multipart call: FLAC in, {"text": ...} out. Serves google/chirp-3
(default) and openai/gpt-4o-transcribe (STT_BACKEND=gpt4o) — multipart
because OpenRouter's JSON input_audio path hangs indefinitely for the
gpt-4o models on >1s audio (probed live 2026-07-17; chirp works on both).
OpenRouter limits (probed live 2026-07-16): response_format=json only — no
timestamps and no confidence fields — and the `language` request param is
ignored (models auto-detect internally but the detection isn't echoed back).
The plain text is split into sentence-shaped segments with pro-rated
timestamps (see _segments), labeled with CHIRP_LANGUAGE_LABEL when the
caller doesn't pin a language.
"""
import io
import json
import os
import re
import urllib.error
import urllib.request
import uuid
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
# chirp-3 answers in ~2-5s; gpt-4o-transcribe swings 1-80s+ on OpenRouter
# (measured 2026-07-17), so gpt4o deploys may need this raised.
TIMEOUT_S = float(os.getenv("OPENROUTER_TIMEOUT_S", "60"))


def _segments(text: str, duration: float) -> List[Dict[str, Any]]:
    """Sentence-shaped segments with timestamps pro-rated by character share.

    Chirp returns no timestamps, and without per-segment ends the confirm
    loop can't advance mid-turn: windows grow unboundedly and text only
    confirms at turn close (12–28s latency measured live). Splitting the
    text lets LocalAgreement confirm stable leading segments continuously.
    Pro-rated boundaries are approximate — the confirm loop's stability
    requirement and the collector's dedup absorb the seam error.
    """
    pieces = [p.strip() for p in re.findall(r"[^.!?…]+[.!?…]*", text) if p.strip()]
    # Long unpunctuated runs (counting, dictation) also need confirmable
    # leading segments: cap every piece at 12 words (~4s of speech).
    split: List[str] = []
    for p in pieces:
        words = p.split()
        for i in range(0, len(words), 12):
            split.append(" ".join(words[i:i + 12]))
    total = sum(len(p) for p in split) or 1
    segs: List[Dict[str, Any]] = []
    t = 0.0
    for i, p in enumerate(split):
        end = duration if i == len(split) - 1 else t + duration * len(p) / total
        segs.append({
            "id": i, "seek": 0, "start": round(t, 3), "end": round(end, 3),
            "text": p, "tokens": [], "temperature": 0.0,
            "audio_start": round(t, 3), "audio_end": round(end, 3),
        })
        t = end
    return segs


def transcribe(audio: np.ndarray, sample_rate: int, language: Optional[str] = None,
               model: Optional[str] = None) -> Dict[str, Any]:
    """One OpenRouter round-trip. Returns the Whisper-shaped response dict."""
    rms = float(np.sqrt(np.mean(audio ** 2))) if audio.size else 0.0
    if 0.001 < rms < TARGET_RMS:
        # ponytail: hard clip after gain; a limiter is overkill for ASR input.
        audio = np.clip(audio * (TARGET_RMS / rms), -1.0, 1.0)

    buf = io.BytesIO()
    sf.write(buf, audio, sample_rate, format="FLAC", subtype="PCM_16")
    boundary = uuid.uuid4().hex
    payload = (
        (
            f"--{boundary}\r\n"
            'Content-Disposition: form-data; name="model"\r\n\r\n'
            f"{model or MODEL}\r\n"
            f"--{boundary}\r\n"
            'Content-Disposition: form-data; name="file"; filename="audio.flac"\r\n'
            "Content-Type: audio/flac\r\n\r\n"
        ).encode("utf-8")
        + buf.getvalue()
        + f"\r\n--{boundary}--\r\n".encode("utf-8")
    )
    req = urllib.request.Request(URL, data=payload, headers={
        "Content-Type": f"multipart/form-data; boundary={boundary}",
        "Authorization": f"Bearer {API_KEY}",
    })
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
            data = json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="replace")[:500]
        # 429/503 stay retryable for the bot's transcription client; anything
        # else (bad key, bad payload) must fail fast, not retry-loop.
        status = e.code if e.code in (429, 503) else 502
        raise HTTPException(status_code=status, detail=f"OpenRouter {e.code}: {body}")

    text = (data.get("text") or "").strip()
    duration = len(audio) / float(sample_rate) if sample_rate else 0.0
    segments = _segments(text, duration) if text else []
    return {
        "text": text,
        "language": language or LANGUAGE_LABEL,
        "language_probability": 0.0,
        "duration": duration,
        "segments": segments,
    }
