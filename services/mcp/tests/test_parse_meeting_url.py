"""
Unit tests for _parse_meeting_url.

Run with: cd services/mcp && pytest tests/test_parse_meeting_url.py -v
"""

import sys
import os
import types
import pytest

# Add parent directory to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))


# Stub out fastapi_mcp before importing main
class _FakeServer:
    def __init__(self):
        self._prompts = {}

    def list_prompts(self):
        def decorator(f):
            self._prompts["list"] = f
            return f

        return decorator

    def get_prompt(self):
        def decorator(f):
            self._prompts["get"] = f
            return f

        return decorator


class _FakeMCP:
    def __init__(self, *a, **kw):
        self.server = _FakeServer()

    def mount_http(self, *a, **kw):
        pass


if "fastapi_mcp" not in sys.modules:
    stub = types.ModuleType("fastapi_mcp")
    stub.FastApiMCP = _FakeMCP
    sys.modules["fastapi_mcp"] = stub

# Stub mcp.types
if "mcp" not in sys.modules:
    mcp_pkg = types.ModuleType("mcp")
    mcp_types = types.ModuleType("mcp.types")

    class _FakePromptArg:
        def __init__(self, **kw):
            pass

    class _FakePrompt:
        def __init__(self, **kw):
            pass

    class _FakeTextContent:
        def __init__(self, **kw):
            pass

    class _FakePromptMessage:
        def __init__(self, **kw):
            pass

    class _FakeListPromptsResult:
        def __init__(self, **kw):
            pass

    class _FakeGetPromptResult:
        def __init__(self, **kw):
            pass

    mcp_types.Prompt = _FakePrompt
    mcp_types.PromptArgument = _FakePromptArg
    mcp_types.TextContent = _FakeTextContent
    mcp_types.PromptMessage = _FakePromptMessage
    mcp_types.ListPromptsResult = _FakeListPromptsResult
    mcp_types.GetPromptResult = _FakeGetPromptResult

    mcp_pkg.types = mcp_types
    sys.modules["mcp"] = mcp_pkg
    sys.modules["mcp.types"] = mcp_types

from fastapi import HTTPException  # noqa: E402
from main import _parse_meeting_url  # noqa: E402


class TestParseMeetingUrl:
    """Tests for the meeting URL parser."""

    def test_google_meet_full_url(self):
        result = _parse_meeting_url("https://meet.google.com/abc-defg-hij")
        assert result.platform == "google_meet"
        assert result.native_meeting_id == "abc-defg-hij"

    def test_zoom_standard_url(self):
        result = _parse_meeting_url("https://zoom.us/j/85173157171")
        assert result.platform == "zoom"
        assert result.native_meeting_id == "85173157171"

    def test_zoom_subdomain_url(self):
        result = _parse_meeting_url("https://us05web.zoom.us/j/85173157171")
        assert result.platform == "zoom"
        assert result.native_meeting_id == "85173157171"

    def test_invalid_url_raises(self):
        with pytest.raises(HTTPException):
            _parse_meeting_url("https://example.com/not-a-meeting")

    def test_empty_string_raises(self):
        with pytest.raises(HTTPException):
            _parse_meeting_url("")
