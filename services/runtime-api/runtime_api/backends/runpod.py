"""RunPod backend — manages bot pods via RunPod REST API.

Spawns GPU pods per meeting, polls for exit, cleans up.
Uses Redis-backed registry (same pattern as process backend).
"""

from __future__ import annotations

import asyncio
import json
import logging
import time
import uuid
from typing import AsyncIterator, Optional

import httpx

from runtime_api import config
from runtime_api.backends import Backend, ContainerInfo, ContainerSpec
from runtime_api.profiles import get_profile

logger = logging.getLogger("runtime_api.backends.runpod")

RUNPOD_PREFIX = "runtime:runpod:"
MANAGED_LABEL = "runtime.managed"
RUNPOD_API_BASE = "https://rest.runpod.io/v1"
POOL_IDLE_SET = "runtime:pool:idle"
POOL_PROFILE = "meeting"  # warm-pool slots are always plain meeting bots

_STATUS_MAP = {
    "RUNNING": "running",
    "EXITED": "exited",
    "TERMINATED": "exited",
}

_CAPACITY_ERROR_MARKERS = (
    "there are no instances currently available",
    "insufficient capacity",
)


class RunPodBackend(Backend):
    def __init__(self, redis=None):
        self._redis = redis
        self._client: Optional[httpx.AsyncClient] = None
        self._reaper_task: Optional[asyncio.Task] = None
        self._pool_task: Optional[asyncio.Task] = None

    def set_redis(self, redis):
        self._redis = redis

    def _get_client(self) -> httpx.AsyncClient:
        if self._client is None:
            self._client = httpx.AsyncClient(
                base_url=RUNPOD_API_BASE,
                headers={"Authorization": f"Bearer {config.RUNPOD_API_KEY}"},
                timeout=30.0,
            )
        return self._client

    async def startup(self) -> None:
        if not config.RUNPOD_API_KEY:
            raise ValueError("RUNPOD_API_KEY is required for runpod backend")
        self._client = httpx.AsyncClient(
            base_url=RUNPOD_API_BASE,
            headers={"Authorization": f"Bearer {config.RUNPOD_API_KEY}"},
            timeout=30.0,
        )
        try:
            resp = await self._client.get("/pods", params={"computeType": "GPU"})
            resp.raise_for_status()
        except httpx.HTTPStatusError as exc:
            if exc.response.status_code in (401, 403, 404):
                logger.warning(
                    "RunPod API preflight skipped: pod listing returned HTTP %s",
                    exc.response.status_code,
                )
                return
            raise

        logger.info(f"RunPod API connected ({len(resp.json())} GPU pods)")

    async def shutdown(self) -> None:
        if self._reaper_task:
            self._reaper_task.cancel()
        if self._pool_task:
            self._pool_task.cancel()
        if self._client:
            await self._client.aclose()
            self._client = None

    async def create(self, spec: ContainerSpec) -> str:
        if spec.gpu and config.MIN_BOT_POOL > 0 and self._redis:
            claimed = await self._claim_pool_slot(spec)
            if claimed:
                return claimed

        client = self._get_client()

        env = dict(spec.env)
        env["RUNPOD_POD_NAME"] = spec.name

        base_payload: dict = {
            "name": spec.name,
            "imageName": spec.image,
            "containerDiskInGb": config.RUNPOD_CONTAINER_DISK_GB,
            "env": env,
            "ports": ["22/tcp"],
            "supportPublicIp": True,
        }

        if spec.gpu:
            gpu_types = list(config.RUNPOD_GPU_TYPES) or [config.RUNPOD_GPU_TYPE]
            last_capacity_error = ""
            attempted_gpu_types: list[str] = []

            for gpu_type in gpu_types:
                attempted_gpu_types.append(gpu_type)
                payload = {
                    **base_payload,
                    "computeType": "GPU",
                    "gpuCount": 1,
                    "gpuTypeIds": [gpu_type],
                    "cloudType": config.RUNPOD_CLOUD_TYPE,
                    "volumeInGb": 0,
                }

                resp = await client.post("/pods", json=payload)
                if resp.is_success:
                    pod = resp.json()
                    pod_id = pod["id"]
                    logger.info(f"Created RunPod pod {spec.name} ({pod_id}) using GPU {gpu_type}")
                    return await self._record_created_pod(spec, pod_id, gpu_type)

                error_text = self._extract_error_text(resp)
                if self._is_capacity_error(error_text):
                    last_capacity_error = error_text
                    logger.warning(
                        "RunPod GPU %s unavailable for %s: %s",
                        gpu_type,
                        spec.name,
                        error_text,
                    )
                    continue

                resp.raise_for_status()

            attempted = ", ".join(attempted_gpu_types)
            detail = last_capacity_error or "RunPod returned no usable capacity error details"
            raise RuntimeError(
                f"RunPod GPU capacity unavailable for {spec.name}. Tried: {attempted}. "
                f"Last error: {detail}"
            )
        else:
            payload = {
                **base_payload,
                "computeType": "CPU",
                "vcpuCount": 4,
            }

            resp = await client.post("/pods", json=payload)
            resp.raise_for_status()
            pod = resp.json()
            pod_id = pod["id"]

            logger.info(f"Created RunPod pod {spec.name} ({pod_id})")
            return await self._record_created_pod(spec, pod_id, None)

    async def _record_created_pod(
        self,
        spec: ContainerSpec,
        pod_id: str,
        gpu_type: str | None,
    ) -> str:
        pod_data = {
            "pod_id": pod_id,
            "name": spec.name,
            "image": spec.image,
            "labels": {**spec.labels, MANAGED_LABEL: "true"},
            "env_keys": list(spec.env.keys()),
            "created_at": time.time(),
            "status": "pending",
        }
        if gpu_type:
            pod_data["gpu_type"] = gpu_type
        if self._redis:
            await self._redis.set(
                f"{RUNPOD_PREFIX}{spec.name}",
                json.dumps(pod_data),
            )

        return pod_id

    @staticmethod
    def _extract_error_text(resp: httpx.Response) -> str:
        try:
            payload = resp.json()
        except ValueError:
            payload = None

        if isinstance(payload, dict):
            for key in ("message", "error", "detail"):
                value = payload.get(key)
                if isinstance(value, str) and value.strip():
                    return value.strip()
            errors = payload.get("errors")
            if isinstance(errors, list):
                parts = [str(item).strip() for item in errors if str(item).strip()]
                if parts:
                    return "; ".join(parts)

        text = resp.text.strip()
        if text:
            return text
        return f"HTTP {resp.status_code}"

    @staticmethod
    def _is_capacity_error(error_text: str) -> bool:
        text = error_text.lower()
        return any(marker in text for marker in _CAPACITY_ERROR_MARKERS)

    async def stop(self, name: str, timeout: int = 10) -> bool:
        data = await self._get_pod_data(name)
        if not data:
            return True

        pod_id = data.get("pod_id")
        if not pod_id:
            return True

        client = self._get_client()
        try:
            resp = await client.post(f"/pods/{pod_id}/stop")
            if resp.status_code == 404:
                return True
            resp.raise_for_status()
        except httpx.HTTPStatusError as e:
            logger.warning(f"Stop pod {name} failed: {e}")
            return False

        if self._redis:
            data["status"] = "stopped"
            data["stopped_at"] = time.time()
            await self._redis.set(f"{RUNPOD_PREFIX}{name}", json.dumps(data), ex=86400)

        return True

    async def remove(self, name: str) -> bool:
        data = await self._get_pod_data(name)
        if not data:
            return True

        pod_id = data.get("pod_id")
        if pod_id:
            client = self._get_client()
            try:
                resp = await client.delete(f"/pods/{pod_id}")
                if resp.status_code not in (204, 404):
                    logger.warning(f"Delete pod {name}: HTTP {resp.status_code}")
            except Exception as e:
                logger.warning(f"Delete pod {name} failed: {e}")

        if self._redis:
            await self._redis.delete(f"{RUNPOD_PREFIX}{name}")
        return True

    async def inspect(self, name: str) -> Optional[ContainerInfo]:
        data = await self._get_pod_data(name)
        if not data:
            return None

        pod_id = data.get("pod_id")
        if not pod_id:
            return None

        client = self._get_client()
        try:
            resp = await client.get(f"/pods/{pod_id}")
            if resp.status_code == 404:
                data["status"] = "exited"
                if self._redis:
                    await self._redis.set(f"{RUNPOD_PREFIX}{name}", json.dumps(data), ex=86400)
                return ContainerInfo(
                    id=pod_id, name=name, status="exited",
                    labels=data.get("labels", {}),
                    created_at=data.get("created_at"),
                    image=data.get("image", ""),
                )
            resp.raise_for_status()
            pod = resp.json()
        except Exception as e:
            logger.warning(f"Inspect pod {name} failed: {e}")
            return ContainerInfo(
                id=pod_id, name=name, status="unknown",
                labels=data.get("labels", {}),
                created_at=data.get("created_at"),
                image=data.get("image", ""),
            )

        desired = pod.get("desiredStatus", "UNKNOWN")
        status = _STATUS_MAP.get(desired, "pending")
        public_ip = pod.get("publicIp")
        port_mappings = pod.get("portMappings") or {}

        if status != data.get("status"):
            data["status"] = status
            if self._redis:
                await self._redis.set(f"{RUNPOD_PREFIX}{name}", json.dumps(data))

        return ContainerInfo(
            id=pod_id, name=name, status=status,
            labels=data.get("labels", {}),
            created_at=data.get("created_at"),
            image=pod.get("image", data.get("image", "")),
            ip=public_ip,
            ports={str(k): v for k, v in port_mappings.items()},
        )

    async def list(self, labels: dict[str, str] | None = None) -> list[ContainerInfo]:
        if not self._redis:
            return []

        results = []
        async for key in self._redis.scan_iter(f"{RUNPOD_PREFIX}*"):
            raw = await self._redis.get(key)
            if not raw:
                continue
            data = json.loads(raw)

            if labels:
                pod_labels = data.get("labels", {})
                if not all(pod_labels.get(k) == v for k, v in labels.items()):
                    continue

            name = data.get("name", key.removeprefix(RUNPOD_PREFIX))
            results.append(ContainerInfo(
                id=data.get("pod_id", name),
                name=name,
                status=data.get("status", "unknown"),
                labels=data.get("labels", {}),
                created_at=data.get("created_at"),
                image=data.get("image", ""),
            ))
        return results

    async def exec(self, name: str, cmd: list[str]) -> AsyncIterator[bytes]:
        logger.warning(f"exec not supported for RunPod backend (pod {name})")
        return
        yield

    async def listen_events(self, on_exit: callable) -> None:
        self._reaper_task = asyncio.create_task(self._reaper_loop(on_exit))
        if config.MIN_BOT_POOL > 0 and self._redis:
            self._pool_task = asyncio.create_task(self._pool_loop())

    async def _reaper_loop(self, on_exit: callable) -> None:
        while True:
            try:
                await asyncio.sleep(config.RUNPOD_POLL_INTERVAL)
                await self._reap_dead(on_exit)
                await self._reconcile_orphans()
            except asyncio.CancelledError:
                return
            except Exception:
                logger.debug("Reaper loop error", exc_info=True)

    # Matches runtime-api's `f"{profile}-{identifier}-{suffix}"` naming
    # (api.py) for every profile in profiles.yaml, plus our own pool prefix.
    _BOT_NAME_PREFIXES = ("meeting-", "browser-session-", "agent-", "pool-")

    async def _reconcile_orphans(self) -> None:
        """Safety net for pods that exited but were never removed because our
        Redis registry lost track of them (e.g. the stateful pod — which also
        hosts Redis — was itself recreated mid-meeting, wiping the tracking
        key). `_reap_dead` only walks keys it still has in Redis, so a pod
        that falls out of the registry is invisible to it forever even after
        it exits on its own. This lists every pod on the account instead and
        deletes any bot pod (matched by our name prefixes) already
        EXITED/TERMINATED on RunPod's side, tracked or not — safe because a
        pod that's already dead can't be mid-meeting."""
        if not self._redis:
            return
        client = self._get_client()
        try:
            resp = await client.get("/pods")
            resp.raise_for_status()
        except Exception:
            logger.debug("Orphan reconcile: failed to list pods", exc_info=True)
            return

        for pod in resp.json():
            name = pod.get("name", "")
            if not name.startswith(self._BOT_NAME_PREFIXES):
                continue
            if pod.get("desiredStatus") not in ("EXITED", "TERMINATED"):
                continue

            pod_id = pod.get("id")
            if not pod_id:
                continue
            try:
                del_resp = await client.delete(f"/pods/{pod_id}")
                if del_resp.status_code in (204, 404):
                    logger.info(f"Orphan reconcile: removed dead untracked-or-stale pod {name} ({pod_id})")
                else:
                    logger.warning(f"Orphan reconcile: delete {name} ({pod_id}) -> HTTP {del_resp.status_code}")
            except Exception:
                logger.warning(f"Orphan reconcile: delete failed for {name} ({pod_id})", exc_info=True)

            await self._redis.srem(POOL_IDLE_SET, name)
            await self._redis.delete(f"{RUNPOD_PREFIX}{name}")

    async def _pool_loop(self) -> None:
        """Keep MIN_BOT_POOL idle bot pods warm (image already pulled, waiting
        on a real assignment via pool-wait.js) so create() can claim one
        instantly instead of cold-spawning + waiting on a fresh RunPod pull."""
        await self._ensure_pool()  # top up immediately on startup
        while True:
            try:
                await asyncio.sleep(config.RUNPOD_POLL_INTERVAL)
                await self._ensure_pool()
            except asyncio.CancelledError:
                return
            except Exception:
                logger.warning("Pool loop error", exc_info=True)

    async def _ensure_pool(self) -> None:
        """Top up the idle pool to MIN_BOT_POOL, pruning dead entries first so
        a crashed slot doesn't permanently count against the target size."""
        await self._prune_pool()
        current = await self._redis.scard(POOL_IDLE_SET)
        missing = config.MIN_BOT_POOL - current
        if missing > 0:
            logger.info(f"Pool below target ({current}/{config.MIN_BOT_POOL}) — spawning {missing} idle slot(s)")
            await asyncio.gather(*(self._spawn_idle_slot() for _ in range(missing)), return_exceptions=True)

    async def _prune_pool(self) -> None:
        """Drop idle-set entries whose backing pod no longer exists or has exited."""
        names = await self._redis.smembers(POOL_IDLE_SET)
        for name in names:
            raw = await self._redis.get(f"{RUNPOD_PREFIX}{name}")
            if not raw:
                await self._redis.srem(POOL_IDLE_SET, name)
                continue
            data = json.loads(raw)
            pod_id = data.get("pod_id")
            if not pod_id:
                await self._redis.srem(POOL_IDLE_SET, name)
                continue
            client = self._get_client()
            try:
                resp = await client.get(f"/pods/{pod_id}")
                if resp.status_code == 404 or (
                    resp.is_success and resp.json().get("desiredStatus") in ("EXITED", "TERMINATED")
                ):
                    await self._redis.srem(POOL_IDLE_SET, name)
                    await self._redis.delete(f"{RUNPOD_PREFIX}{name}")
            except Exception:
                logger.debug(f"Pool prune: failed to check {name}", exc_info=True)

    async def _spawn_idle_slot(self) -> Optional[str]:
        """Create one pre-warmed idle bot pod. Uses the 'meeting' profile's
        image/resources/gpu settings — pool slots are always plain meeting
        bots; agent/browser-session profiles aren't pooled."""
        profile_def = get_profile(POOL_PROFILE)
        if not profile_def:
            logger.warning(f"Pool: profile '{POOL_PROFILE}' not found, skipping spawn")
            return None

        pool_name = f"pool-{uuid.uuid4().hex[:12]}"
        env = {
            "POOL_SLOT": "true",
            "POOL_REDIS_URL": config.BOT_REDIS_URL,
            "RUNPOD_POD_NAME": pool_name,
            **profile_def.get("env", {}),
        }
        client = self._get_client()
        base_payload: dict = {
            "name": pool_name,
            "imageName": profile_def["image"],
            "containerDiskInGb": config.RUNPOD_CONTAINER_DISK_GB,
            "env": env,
            "ports": ["22/tcp"],
            "supportPublicIp": True,
        }
        gpu_types = list(config.RUNPOD_GPU_TYPES) or [config.RUNPOD_GPU_TYPE]
        for gpu_type in gpu_types:
            payload = {
                **base_payload,
                "computeType": "GPU",
                "gpuCount": 1,
                "gpuTypeIds": [gpu_type],
                "cloudType": config.RUNPOD_CLOUD_TYPE,
                "volumeInGb": 0,
            }
            try:
                resp = await client.post("/pods", json=payload)
            except Exception as e:
                logger.warning(f"Pool: spawn request failed for {pool_name}: {e}")
                continue
            if resp.is_success:
                pod = resp.json()
                pod_id = pod["id"]
                pool_data = {
                    "pod_id": pod_id,
                    "name": pool_name,
                    "image": profile_def["image"],
                    "labels": {MANAGED_LABEL: "true", "runtime.pool": "true"},
                    "env_keys": list(env.keys()),
                    "created_at": time.time(),
                    "status": "pool_idle",
                    "gpu_type": gpu_type,
                }
                await self._redis.set(f"{RUNPOD_PREFIX}{pool_name}", json.dumps(pool_data))
                await self._redis.sadd(POOL_IDLE_SET, pool_name)
                logger.info(f"Pool: spawned idle slot {pool_name} ({pod_id}) using GPU {gpu_type}")
                return pod_id

            error_text = self._extract_error_text(resp)
            if not self._is_capacity_error(error_text):
                logger.warning(f"Pool: spawn failed for {pool_name}: {error_text}")
                return None
        logger.warning(f"Pool: no GPU capacity available across {gpu_types} for {pool_name}")
        return None

    async def _claim_pool_slot(self, spec: ContainerSpec) -> Optional[str]:
        """Hand a pre-warmed idle pod a real BOT_CONFIG instead of cold-spawning.
        Returns the claimed pod_id, or None if no slot was available (caller
        falls back to a normal create())."""
        pool_name = await self._redis.spop(POOL_IDLE_SET)
        if not pool_name:
            return None
        raw = await self._redis.get(f"{RUNPOD_PREFIX}{pool_name}")
        if not raw:
            return None
        data = json.loads(raw)
        pod_id = data.get("pod_id")
        if not pod_id:
            return None

        bot_config_str = spec.env.get("BOT_CONFIG", "{}")
        await self._redis.rpush(f"pool:assign:{pod_id}", bot_config_str)

        # Re-key under the real meeting name so stop/inspect/list work exactly
        # as if this pod had been created fresh under spec.name.
        new_data = {
            **data,
            "name": spec.name,
            "labels": {**spec.labels, MANAGED_LABEL: "true"},
            "env_keys": list(spec.env.keys()),
            "status": "pending",
            "claimed_at": time.time(),
        }
        await self._redis.set(f"{RUNPOD_PREFIX}{spec.name}", json.dumps(new_data))
        await self._redis.delete(f"{RUNPOD_PREFIX}{pool_name}")

        logger.info(f"Pool: claimed idle slot {pool_name} ({pod_id}) for {spec.name} — no cold RunPod spawn")
        asyncio.create_task(self._top_up_pool())
        return pod_id

    async def _top_up_pool(self) -> None:
        """Spawn one replacement idle pod after a claim, keeping pool size steady."""
        try:
            await self._spawn_idle_slot()
        except Exception as e:
            logger.warning(f"Pool top-up failed: {e}")

    async def _reap_dead(self, on_exit: callable) -> None:
        if not self._redis:
            return

        async for key in self._redis.scan_iter(f"{RUNPOD_PREFIX}*"):
            raw = await self._redis.get(key)
            if not raw:
                continue
            data = json.loads(raw)
            if data.get("status") not in ("running", "pending"):
                continue

            pod_id = data.get("pod_id")
            if not pod_id:
                continue

            name = data.get("name", key.removeprefix(RUNPOD_PREFIX))
            client = self._get_client()

            try:
                resp = await client.get(f"/pods/{pod_id}")
                if resp.status_code == 404:
                    exit_code = 0
                    data["status"] = "exited"
                    data["stopped_at"] = time.time()
                    data["exit_code"] = exit_code
                    await self._redis.set(key, json.dumps(data), ex=86400)
                    if on_exit:
                        await on_exit(name, exit_code)
                    continue

                resp.raise_for_status()
                pod = resp.json()
                desired = pod.get("desiredStatus", "UNKNOWN")

                if desired in ("EXITED", "TERMINATED"):
                    exit_code = 0
                    data["status"] = "exited"
                    data["stopped_at"] = time.time()
                    data["exit_code"] = exit_code
                    await self._redis.set(key, json.dumps(data), ex=86400)
                    logger.info(f"Reaper: pod {name} ({pod_id}) {desired.lower()}")
                    if on_exit:
                        await on_exit(name, exit_code)
            except Exception:
                logger.debug(f"Failed to check pod {name}", exc_info=True)

    async def _get_pod_data(self, name: str) -> Optional[dict]:
        if not self._redis:
            return None
        raw = await self._redis.get(f"{RUNPOD_PREFIX}{name}")
        if raw:
            return json.loads(raw)
        return None
