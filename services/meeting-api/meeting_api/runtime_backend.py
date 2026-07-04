"""Local-vs-RunPod runtime-api backend selection.

Replaces the standalone `router` service. Router special-cased `POST /bots` /
`DELETE /bots/{platform}/{id}` with a sticky, in-memory local/RunPod split — but nothing ever
called those routes (runtime-api only exposes /containers, not /bots), so real traffic went
through router's catch-all proxy instead, which routed on the single static USE_LOCAL_RESOURCE
env var only. There was no actual per-bot load balancing happening.

This picks a backend once per meeting at spawn time based on current local-bot occupancy vs
LOCAL_BOT_THRESHOLD, persists the choice in meeting.data (survives restarts — the in-memory map
didn't), and resolves it back out for every later container operation on that meeting.
"""

from sqlalchemy import and_, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from .config import LOCAL_BACKEND_URL, LOCAL_BOT_THRESHOLD, RUNPOD_BACKEND_URL, USE_LOCAL_RESOURCE
from .models import Meeting

ACTIVE_STATUSES = ["requested", "joining", "awaiting_admission", "active"]


async def choose_backend_for_spawn(db: AsyncSession) -> str:
    """Pick "local" or "runpod" for a *new* bot about to be spawned."""
    if not USE_LOCAL_RESOURCE:
        return "runpod"

    count_stmt = select(func.count()).select_from(Meeting).where(
        and_(
            Meeting.platform != "browser_session",
            Meeting.status.in_(ACTIVE_STATUSES),
            Meeting.data["runtime_backend"].astext == "local",
        )
    )
    local_count = int((await db.execute(count_stmt)).scalar() or 0)
    return "local" if local_count < LOCAL_BOT_THRESHOLD else "runpod"


def backend_url(backend: str) -> str:
    return LOCAL_BACKEND_URL if backend == "local" else RUNPOD_BACKEND_URL


def meeting_backend_url(meeting: Meeting) -> str:
    """Resolve the backend URL already decided for an existing meeting. Falls back to the
    static USE_LOCAL_RESOURCE flag only for meetings created before this field existed."""
    backend = (meeting.data or {}).get("runtime_backend")
    if backend is None:
        backend = "local" if USE_LOCAL_RESOURCE else "runpod"
    return backend_url(backend)
