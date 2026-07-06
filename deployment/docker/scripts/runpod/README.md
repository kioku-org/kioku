# RunPod deploy

Deploys the `kioku-stateful` pod on RunPod via `runpodctl`. `kioku-stateless` bot pods
aren't deployed separately — `runtime-api-runpod` (running inside the stateful pod)
spawns them automatically on RunPod when a meeting is requested, using the
`RUNPOD_GPU_TYPES`/`BOT_IMAGE` config below.

## Prerequisites

- [`runpodctl`](https://github.com/runpod/runpodctl) installed and on `PATH`
- A RunPod API key

## Usage

```bash
cp .env.example .env   # fill in the required values
./deploy.sh
./destroy.sh <pod-id>  # tear down when done
```

## Known risk: image pull may need registry auth

`ghcr.io/kioku-org/kioku-stateful`/`kioku-stateless` are the images CI actually
builds and pushes (the previous defaults pointed at a stale, unrelated `kyomoto/*`
registry — fixed). Whether these GHCR packages are public or private wasn't
confirmed — verify at https://github.com/orgs/kioku-org/packages before relying on
this script, since RunPod's `runpodctl pod create` has no way to pass a registry
pull secret. If the packages turn out to be private, either flip them to public in
GitHub's package settings, or push a public mirror somewhere RunPod can pull from
unauthenticated.
