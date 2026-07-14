"""Shared-whisper transcriber pool.

One whisper instance ("shard") serves up to BOTS_PER_TRANSCRIBER concurrent
meetings on its backend (local Docker or RunPod); the first spawn past that
capacity starts a new shard via runtime-api (profile "whisper"). Shards with
no active meetings are reaped by the sweep in sweeps.py.

Disabled unless BOTS_PER_TRANSCRIBER > 0 — and on any failure to provision a
shard, assign_transcriber returns (None, None, None) so the bot falls back to
the pre-pool behavior (BOT_TRANSCRIPTION_SERVICE_URL env or its in-pod model).

Shard accounting is derived from the meetings table (data.transcriber_shard,
written at spawn), same pattern as choose_runtime_backend — no separate
counter state to drift.
"""

import logging
import os
from datetime import datetime, timezone
from typing import Optional, Tuple

import httpx
from sqlalchemy import func, select
from sqlalchemy.ext.asyncio import AsyncSession

from .config import LOCAL_BACKEND_URL, RUNPOD_BACKEND_URL
from .models import Meeting

logger = logging.getLogger(__name__)

BOTS_PER_TRANSCRIBER = int(os.environ.get("BOTS_PER_TRANSCRIBER", "0") or 0)

# Same active set choose_runtime_backend uses for backend routing.
ACTIVE_BOT_STATUSES = ["requested", "joining", "awaiting_admission", "active"]

# Don't reap a shard younger than this — its first meeting may still be in
# "requested" limbo or the row commit may race the sweep.
REAP_GRACE_SECONDS = 300


def _shard_name(backend_name: str, idx: int) -> str:
    return f"kioku-whisper-{backend_name}-{idx}"


def _shard_url(backend_name: str, name: str, info: Optional[dict]) -> Optional[str]:
    if backend_name == "runpod":
        pod_id = ((info or {}).get("metadata") or {}).get("pod_id")
        # RunPod's HTTP proxy; requires the pod to expose 8000/http.
        return f"https://{pod_id}-8000.proxy.runpod.net" if pod_id else None
    # Local shards share kioku-network with the bots — container-name DNS.
    return f"http://{name}:8000"


async def _shard_counts(db: AsyncSession, backend_name: str) -> dict:
    shard = Meeting.data["transcriber_shard"].astext
    rows = await db.execute(
        select(shard, func.count())
        .where(
            Meeting.status.in_(ACTIVE_BOT_STATUSES),
            Meeting.platform != "browser_session",
            Meeting.data["runtime_backend"].astext == backend_name,
            shard.isnot(None),
        )
        .group_by(shard)
    )
    return {name: count for name, count in rows.all()}


async def _get_shard(name: str, backend_url: str) -> Optional[dict]:
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            resp = await client.get(f"{backend_url}/containers/{name}")
        if resp.status_code == 200:
            return resp.json()
    except httpx.RequestError as e:
        logger.warning(f"transcriber pool: GET {name} failed: {e}")
    return None


async def _create_shard(name: str, backend_url: str) -> Optional[dict]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            resp = await client.post(
                f"{backend_url}/containers",
                json={"profile": "whisper", "name": name, "user_id": "system", "config": {}},
            )
        if resp.status_code == 201:
            return resp.json()
        # Lost a create race — someone else made it; fetch it.
        logger.info(f"transcriber pool: create {name} returned {resp.status_code}, re-fetching")
        return await _get_shard(name, backend_url)
    except httpx.RequestError as e:
        logger.error(f"transcriber pool: create {name} failed: {e}")
    return None


async def assign_transcriber(
    db: AsyncSession, backend_url: str, backend_name: str
) -> Tuple[Optional[str], Optional[str], Optional[str]]:
    """Pick (or start) a whisper shard with capacity for one more meeting.

    Returns (url, shard_name, token); all None when the pool is disabled or
    provisioning failed (caller keeps legacy transcription behavior).
    """
    if BOTS_PER_TRANSCRIBER <= 0:
        return None, None, None

    counts = await _shard_counts(db, backend_name)
    idx = 0
    while counts.get(_shard_name(backend_name, idx), 0) >= BOTS_PER_TRANSCRIBER:
        idx += 1
    name = _shard_name(backend_name, idx)

    info = await _get_shard(name, backend_url)
    if not info or info.get("status") in ("stopped", "exited", "dead"):
        info = await _create_shard(name, backend_url)
    if not info:
        return None, None, None

    url = _shard_url(backend_name, name, info)
    if not url:
        return None, None, None

    # The whisper profile sets API_TOKEN from INTERNAL_API_SECRET, so every
    # bot must present it — local shards enforce it too when the secret is set.
    token = os.getenv("INTERNAL_API_SECRET") or None
    logger.info(f"transcriber pool: meeting assigned to {name} ({url})")
    return url, name, token


def _created_age_seconds(info: dict) -> Optional[float]:
    raw = info.get("created_at")
    if raw is None:
        return None
    try:
        if isinstance(raw, (int, float)):
            created = datetime.fromtimestamp(float(raw), tz=timezone.utc)
        else:
            created = datetime.fromisoformat(str(raw).replace("Z", "+00:00"))
            if created.tzinfo is None:
                created = created.replace(tzinfo=timezone.utc)
        return (datetime.now(timezone.utc) - created).total_seconds()
    except (ValueError, OSError):
        return None


async def sweep_idle_transcribers(db: AsyncSession) -> int:
    """Stop whisper shards that serve no active meeting (RunPod ones cost money)."""
    if BOTS_PER_TRANSCRIBER <= 0:
        return 0
    reaped = 0
    for backend_name, backend_url in (("local", LOCAL_BACKEND_URL), ("runpod", RUNPOD_BACKEND_URL)):
        counts = await _shard_counts(db, backend_name)
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                resp = await client.get(f"{backend_url}/containers", params={"profile": "whisper"})
            shards = resp.json() if resp.status_code == 200 else []
        except httpx.RequestError:
            continue
        for shard in shards:
            name = shard.get("name", "")
            if counts.get(name, 0) > 0:
                continue
            age = _created_age_seconds(shard)
            if age is None or age < REAP_GRACE_SECONDS:
                continue
            try:
                async with httpx.AsyncClient(timeout=30.0) as client:
                    await client.delete(f"{backend_url}/containers/{name}")
                logger.info(f"transcriber pool: reaped idle shard {name} ({backend_name})")
                reaped += 1
            except httpx.RequestError as e:
                logger.warning(f"transcriber pool: reap {name} failed: {e}")
    return reaped
