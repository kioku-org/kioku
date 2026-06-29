import asyncio
import json
import os
from typing import Dict

import httpx
from fastapi import FastAPI, Request, Response

USE_LOCAL_RESOURCE = os.getenv("USE_LOCAL_RESOURCE", "true").lower() == "true"
LOCAL_BOT_THRESHOLD = int(os.getenv("LOCAL_BOT_THRESHOLD", "3"))
LOCAL_URL = os.getenv("LOCAL_BACKEND_URL", "http://vexa-runtime-api-local:8090")
RUNPOD_URL = os.getenv("RUNPOD_BACKEND_URL", "http://vexa-runtime-api-runpod:8090")

app = FastAPI(docs_url=None, redoc_url=None, openapi_url=None)

_lock = asyncio.Lock()
_local_count: int = 0
_bot_backends: Dict[str, str] = {}  # "platform:meeting_id" -> "local" | "runpod"


@app.get("/health")
async def health():
    return {
        "status": "ok",
        "use_local_resource": USE_LOCAL_RESOURCE,
        "local_bot_threshold": LOCAL_BOT_THRESHOLD,
        "local_bot_count": _local_count,
        "tracked_bots": len(_bot_backends),
    }


async def _forward(method: str, url: str, request: Request, body: bytes) -> Response:
    async with httpx.AsyncClient(timeout=60) as client:
        headers = {k: v for k, v in request.headers.items() if k.lower() != "host"}
        resp = await client.request(
            method=method,
            url=url,
            headers=headers,
            content=body,
            params=dict(request.query_params),
        )
    return Response(
        content=resp.content,
        status_code=resp.status_code,
        media_type=resp.headers.get("content-type"),
    )


@app.post("/bots")
async def spawn_bot(request: Request) -> Response:
    global _local_count
    body = await request.body()
    try:
        payload = json.loads(body) if body else {}
    except Exception:
        payload = {}

    platform = payload.get("platform", "unknown")
    meeting_id = payload.get("native_meeting_id", "unknown")
    bot_key = f"{platform}:{meeting_id}"

    async with _lock:
        use_local = USE_LOCAL_RESOURCE and _local_count < LOCAL_BOT_THRESHOLD
        backend = "local" if use_local else "runpod"
        target = LOCAL_URL if use_local else RUNPOD_URL

    resp = await _forward("POST", f"{target}/bots", request, body)

    if resp.status_code < 300:
        async with _lock:
            _bot_backends[bot_key] = backend
            if backend == "local":
                _local_count += 1

    return resp


@app.delete("/bots/{platform}/{meeting_id}")
async def stop_bot(platform: str, meeting_id: str, request: Request) -> Response:
    global _local_count
    bot_key = f"{platform}:{meeting_id}"

    async with _lock:
        backend = _bot_backends.get(bot_key, "local" if USE_LOCAL_RESOURCE else "runpod")
    target = LOCAL_URL if backend == "local" else RUNPOD_URL

    body = await request.body()
    resp = await _forward("DELETE", f"{target}/bots/{platform}/{meeting_id}", request, body)

    if resp.status_code < 300:
        async with _lock:
            removed = _bot_backends.pop(bot_key, None)
            if removed == "local" and _local_count > 0:
                _local_count -= 1

    return resp


@app.api_route(
    "/{path:path}",
    methods=["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"],
)
async def proxy(path: str, request: Request) -> Response:
    target = LOCAL_URL if USE_LOCAL_RESOURCE else RUNPOD_URL
    body = await request.body()
    return await _forward(request.method, f"{target}/{path}", request, body)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8090)
