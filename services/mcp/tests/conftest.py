"""conftest.py -- pytest path setup for mcp unit tests."""
import sys
import os
import types

SERVICE_ROOT = os.path.join(os.path.dirname(__file__), "..")
sys.path.insert(0, SERVICE_ROOT)

# Add vexa meeting-api to path for schemas
MEETING_API = os.path.join(os.path.dirname(__file__), "..", "..", "vexa", "services", "meeting-api")
sys.path.insert(0, MEETING_API)

# Stub out fastapi_mcp before importing main
class _FakeServer:
    def __init__(self):
        self._prompts = {}
    
    def list_prompts(self):
        def decorator(f):
            self._prompts['list'] = f
            return f
        return decorator
    
    def get_prompt(self):
        def decorator(f):
            self._prompts['get'] = f
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

# Stub mcp.types if not available
if "mcp" not in sys.modules:
    mcp_pkg = types.ModuleType("mcp")
    mcp_types = types.ModuleType("mcp.types")

    class _FakePromptArg:
        def __init__(self, **kw): pass
    class _FakePrompt:
        def __init__(self, **kw): pass
    class _FakeTextContent:
        def __init__(self, **kw): pass
    class _FakePromptMessage:
        def __init__(self, **kw): pass
    class _FakeListPromptsResult:
        def __init__(self, **kw): pass
    class _FakeGetPromptResult:
        def __init__(self, **kw): pass

    mcp_types.Prompt = _FakePrompt
    mcp_types.PromptArgument = _FakePromptArg
    mcp_types.TextContent = _FakeTextContent
    mcp_types.PromptMessage = _FakePromptMessage
    mcp_types.ListPromptsResult = _FakeListPromptsResult
    mcp_types.GetPromptResult = _FakeGetPromptResult

    mcp_pkg.types = mcp_types
    sys.modules["mcp"] = mcp_pkg
    sys.modules["mcp.types"] = mcp_types
