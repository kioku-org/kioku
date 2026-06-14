import pytest
import requests
import time
import os

BASE_URL = os.environ.get("VEXA_API_URL", "http://localhost:8056")
ADMIN_URL = os.environ.get("VEXA_ADMIN_URL", "http://localhost:8057")
ADMIN_TOKEN = os.environ.get("VEXA_ADMIN_API_TOKEN", "token")


@pytest.fixture(scope="session")
def api_client():
    return requests.Session()


@pytest.fixture(scope="session")
def admin_headers():
    return {"X-API-Key": ADMIN_TOKEN, "Content-Type": "application/json"}


class TestHealthChecks:
    def test_api_gateway_health(self, api_client):
        resp = api_client.get(f"{BASE_URL}/")
        assert resp.status_code == 200, f"API Gateway root failed: {resp.status_code}"

    def test_api_gateway_docs(self, api_client):
        resp = api_client.get(f"{BASE_URL}/docs")
        assert resp.status_code == 200, f"API docs unreachable: {resp.status_code}"

    def test_admin_api_health(self, api_client, admin_headers):
        resp = api_client.get(f"{ADMIN_URL}/admin/health", headers=admin_headers)
        assert resp.status_code in (200, 404), f"Admin API unreachable: {resp.status_code}"


class TestBotManagement:
    def test_list_bots(self, api_client, admin_headers):
        resp = api_client.get(f"{BASE_URL}/bots", headers=admin_headers)
        assert resp.status_code in (200, 401), f"List bots failed: {resp.status_code}"
        if resp.status_code == 200:
            data = resp.json()
            assert isinstance(data, (list, dict)), "Bots response should be list or dict"

    def test_bot_status(self, api_client, admin_headers):
        resp = api_client.get(f"{BASE_URL}/bots/status", headers=admin_headers)
        assert resp.status_code in (200, 401), f"Bot status failed: {resp.status_code}"
        if resp.status_code == 200:
            data = resp.json()
            assert "bots" in data or isinstance(data, dict), "Bot status should contain bots info"

    def test_request_bot_missing_auth(self, api_client):
        resp = api_client.post(f"{BASE_URL}/bots", json={
            "platform": "google_meet",
            "native_meeting_id": "test-123"
        })
        assert resp.status_code == 403, f"Unauthenticated bot request should be 403, got {resp.status_code}"


class TestMeetingTranscripts:
    def test_get_transcript_no_meeting(self, api_client, admin_headers):
        resp = api_client.get(
            f"{BASE_URL}/transcripts/google_meet/nonexistent-meeting",
            headers=admin_headers,
        )
        assert resp.status_code in (200, 404, 403), f"Transcript GET failed: {resp.status_code}"

    def test_get_meetings(self, api_client, admin_headers):
        resp = api_client.get(f"{BASE_URL}/meetings", headers=admin_headers)
        assert resp.status_code in (200, 401), f"Get meetings failed: {resp.status_code}"


class TestAdminAPI:
    def test_admin_meetings(self, api_client, admin_headers):
        resp = api_client.get(f"{ADMIN_URL}/admin/meetings", headers=admin_headers)
        assert resp.status_code in (200, 404), f"Admin meetings failed: {resp.status_code}"

    def test_admin_api_keys_invalid(self, api_client):
        resp = api_client.get(f"{ADMIN_URL}/admin/meetings", headers={"X-API-Key": "invalid"})
        assert resp.status_code in (401, 403), f"Invalid key should be rejected: {resp.status_code}"


class TestWebSocket:
    def test_ws_connect_no_auth(self):
        try:
            import websocket
            ws_url = BASE_URL.replace("http", "ws") + "/ws"
            with pytest.raises(Exception):
                ws = websocket.create_connection(ws_url)
                ws.close()
        except ImportError:
            pytest.skip("websocket-client not installed")


class TestMCP:
    def test_mcp_health(self, api_client):
        mcp_url = os.environ.get("VEXA_MCP_URL", "http://localhost:18888")
        try:
            resp = api_client.get(f"{mcp_url}/health", timeout=3)
            assert resp.status_code in (200, 404), f"MCP service unreachable: {resp.status_code}"
        except requests.ConnectionError:
            pytest.skip("MCP service not available")