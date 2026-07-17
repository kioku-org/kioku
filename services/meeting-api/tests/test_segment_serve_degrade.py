"""Issue #108: stored segments with invalid metadata must serve degraded, not drop."""
from meeting_api.schemas import TranscriptionSegment


def test_invalid_language_is_coerced_not_rejected():
    seg = TranscriptionSegment(start=0.0, end=1.5, text="hello", language="unknown")
    assert seg.text == "hello"
    assert seg.language is None


def test_valid_language_passes_through():
    seg = TranscriptionSegment(start=0.0, end=1.5, text="hello", language="en")
    assert seg.language == "en"
