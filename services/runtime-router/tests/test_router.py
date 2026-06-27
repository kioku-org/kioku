import os

import pytest
import pytest_asyncio
from httpx import ASGITransport, AsyncClient

os.environ.setdefault("USE_LOCAL_RESOURCE", "true")
os.environ.setdefault("LOCAL_BOT_THRESHOLD", "3")
os.environ.setdefault("LOCAL_BACKEND_URL", "http://local-backend:8090")
os.environ.setdefault("RUNPOD_BACKEND_URL", "http://runpod-backend:8090")

import main  # noqa: E402 — env must be set before import


@pytest.fixture
def reset_state():
    main._local_count = 0
    main._bot_backends.clear()
    yield
    main._local_count = 0
    main._bot_backends.clear()


@pytest.mark.asyncio
async def test_health(reset_state):
    async with AsyncClient(transport=ASGITransport(app=main.app), base_url="http://test") as client:
        resp = await client.get("/health")
    assert resp.status_code == 200
    data = resp.json()
    assert data["status"] == "ok"
    assert data["local_bot_count"] == 0
    assert data["tracked_bots"] == 0
    assert "use_local_resource" in data
    assert "local_bot_threshold" in data


@pytest.mark.asyncio
async def test_routing_local_when_under_threshold(reset_state):
    main.USE_LOCAL_RESOURCE = True
    main.LOCAL_BOT_THRESHOLD = 3
    main._local_count = 0

    import json
    from unittest.mock import AsyncMock, patch

    mock_resp = AsyncMock()
    mock_resp.status_code = 200
    mock_resp.content = b'{"ok": true}'
    mock_resp.headers = {"content-type": "application/json"}

    chosen_url = []

    async def fake_forward(method, url, request, body):
        chosen_url.append(url)
        from fastapi.responses import JSONResponse
        return JSONResponse({"ok": True})

    with patch.object(main, "_forward", side_effect=fake_forward):
        async with AsyncClient(transport=ASGITransport(app=main.app), base_url="http://test") as client:
            resp = await client.post(
                "/bots",
                content=json.dumps({"platform": "google_meet", "native_meeting_id": "abc-defg-hij"}),
                headers={"content-type": "application/json"},
            )

    assert resp.status_code == 200
    assert main.LOCAL_URL in chosen_url[0]
    assert main._local_count == 1
    assert main._bot_backends.get("google_meet:abc-defg-hij") == "local"


@pytest.mark.asyncio
async def test_routing_runpod_when_at_threshold(reset_state):
    main.USE_LOCAL_RESOURCE = True
    main.LOCAL_BOT_THRESHOLD = 3
    main._local_count = 3  # already at cap

    import json

    chosen_url = []

    async def fake_forward(method, url, request, body):
        chosen_url.append(url)
        from fastapi.responses import JSONResponse
        return JSONResponse({"ok": True})

    with patch.object(main, "_forward", side_effect=fake_forward):
        async with AsyncClient(transport=ASGITransport(app=main.app), base_url="http://test") as client:
            resp = await client.post(
                "/bots",
                content=json.dumps({"platform": "zoom", "native_meeting_id": "12345678901"}),
                headers={"content-type": "application/json"},
            )

    assert resp.status_code == 200
    assert main.RUNPOD_URL in chosen_url[0]
    assert main._local_count == 3  # unchanged
    assert main._bot_backends.get("zoom:12345678901") == "runpod"


@pytest.mark.asyncio
async def test_stop_decrements_local_count(reset_state):
    main.USE_LOCAL_RESOURCE = True
    main._local_count = 2
    main._bot_backends["google_meet:abc-defg-hij"] = "local"

    async def fake_forward(method, url, request, body):
        from fastapi.responses import JSONResponse
        return JSONResponse({"ok": True})

    with patch.object(main, "_forward", side_effect=fake_forward):
        async with AsyncClient(transport=ASGITransport(app=main.app), base_url="http://test") as client:
            resp = await client.delete("/bots/google_meet/abc-defg-hij")

    assert resp.status_code == 200
    assert main._local_count == 1
    assert "google_meet:abc-defg-hij" not in main._bot_backends


@pytest.mark.asyncio
async def test_use_local_false_routes_all_to_runpod(reset_state):
    main.USE_LOCAL_RESOURCE = False
    main._local_count = 0

    import json

    chosen_url = []

    async def fake_forward(method, url, request, body):
        chosen_url.append(url)
        from fastapi.responses import JSONResponse
        return JSONResponse({"ok": True})

    with patch.object(main, "_forward", side_effect=fake_forward):
        async with AsyncClient(transport=ASGITransport(app=main.app), base_url="http://test") as client:
            await client.post(
                "/bots",
                content=json.dumps({"platform": "google_meet", "native_meeting_id": "xxx-yyyy-zzz"}),
                headers={"content-type": "application/json"},
            )

    assert main.RUNPOD_URL in chosen_url[0]
    assert main._local_count == 0

    main.USE_LOCAL_RESOURCE = True  # restore
