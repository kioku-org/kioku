"""Unit tests for transcription-service configuration and pure helper functions.

Tests config parsing, quality heuristics, and tier logic without a running model.
"""
import os
import sys
import pytest

# Add service root to path
SERVICE_ROOT = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, SERVICE_ROOT)

from main import (
    _looks_like_silence,
    _looks_like_hallucination,
    _normalize_transcription_tier,
    _deferred_capacity_available,
    NO_SPEECH_THRESHOLD,
    LOG_PROB_THRESHOLD,
    COMPRESSION_RATIO_THRESHOLD,
)


class TestLooksLikeSilence:
    def test_empty_segments_is_silence(self):
        assert _looks_like_silence([]) is True

    def test_high_no_speech_low_logprob_is_silence(self):
        segments = [{"no_speech_prob": 0.95, "avg_logprob": -2.0}]
        assert _looks_like_silence(segments) is True

    def test_normal_speech_is_not_silence(self):
        segments = [{"no_speech_prob": 0.1, "avg_logprob": -0.3}]
        assert _looks_like_silence(segments) is False

    def test_mixed_segments_not_silence(self):
        """If any segment looks like real speech, not silence."""
        segments = [
            {"no_speech_prob": 0.95, "avg_logprob": -2.0},
            {"no_speech_prob": 0.1, "avg_logprob": -0.3},
        ]
        assert _looks_like_silence(segments) is False


# --- _looks_like_hallucination ---

class TestLooksLikeHallucination:
    def test_high_compression_is_hallucination(self):
        segments = [{"compression_ratio": 3.0, "avg_logprob": -0.5}]
        assert _looks_like_hallucination(segments) is True

    def test_low_logprob_is_hallucination(self):
        segments = [{"compression_ratio": 1.5, "avg_logprob": -2.0}]
        assert _looks_like_hallucination(segments) is True

    def test_normal_segment_not_hallucination(self):
        segments = [{"compression_ratio": 1.5, "avg_logprob": -0.5}]
        assert _looks_like_hallucination(segments) is False

    def test_empty_segments_not_hallucination(self):
        assert _looks_like_hallucination([]) is False


# --- _normalize_transcription_tier ---

class TestNormalizeTranscriptionTier:
    def test_realtime_default(self):
        assert _normalize_transcription_tier(None) == "realtime"

    def test_realtime_explicit(self):
        assert _normalize_transcription_tier("realtime") == "realtime"

    def test_deferred_explicit(self):
        assert _normalize_transcription_tier("deferred") == "deferred"

    def test_unknown_falls_back_to_realtime(self):
        assert _normalize_transcription_tier("batch") == "realtime"

    def test_whitespace_stripped(self):
        assert _normalize_transcription_tier("  deferred  ") == "deferred"

    def test_case_insensitive(self):
        assert _normalize_transcription_tier("DEFERRED") == "deferred"
        assert _normalize_transcription_tier("Realtime") == "realtime"


# --- _deferred_capacity_available ---

class TestDeferredCapacityAvailable:
    def test_capacity_when_empty(self):
        # With default MAX_CONCURRENT_TRANSCRIPTIONS=20, REALTIME_RESERVED_SLOTS=1
        # deferred_limit = 19, both counts 0 -> True
        assert _deferred_capacity_available(0, 0) is True

    def test_no_capacity_when_deferred_full(self):
        from main import MAX_CONCURRENT_TRANSCRIPTIONS, REALTIME_RESERVED_SLOTS
        deferred_limit = max(0, MAX_CONCURRENT_TRANSCRIPTIONS - REALTIME_RESERVED_SLOTS)
        assert _deferred_capacity_available(0, deferred_limit) is False

    def test_no_capacity_when_total_full(self):
        from main import MAX_CONCURRENT_TRANSCRIPTIONS
        assert _deferred_capacity_available(MAX_CONCURRENT_TRANSCRIPTIONS, 0) is False

    def test_has_capacity_with_some_active(self):
        assert _deferred_capacity_available(5, 3) is True
