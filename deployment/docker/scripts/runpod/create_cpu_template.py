#!/usr/bin/env python3
"""Create a CPU/OpenRouter RunPod template from the checked-out deployment files."""
import argparse
import base64
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[4]
IMAGE = "ghcr.io/kioku-org/kioku-stateful@sha256:b1a8ec53617b9ef5c8237a73d2f2c2c8bd0ae667dc9f8de5390f40278be0b896"

def startup_entrypoint():
    files = [
        ("deployment/docker/entrypoint-stateful-runtime.sh", "/entrypoint.sh"),
        ("deployment/docker/configs/runtime-profiles.yaml", "/opt/vexa/services/runtime-api/profiles.yaml"),
    ]
    startup = "\n".join(
        f"printf %s {base64.b64encode((ROOT / source).read_bytes()).decode()} | base64 -d > {target}"
        for source, target in files
    ) + "\nchmod +x /entrypoint.sh\nexec /entrypoint.sh\n"
    encoded = base64.b64encode(startup.encode()).decode()
    return ["/bin/bash", "-c", f'exec /bin/bash -c "$(printf %s {encoded} | base64 -d)"']

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", default="kioku-prod-cpu-openrouter")
    parser.add_argument("--image", default=IMAGE)
    parser.add_argument("--cli", default="runpodctl")
    args = parser.parse_args()
    command = [
        args.cli, "template", "create", "--name", args.name, "--image", args.image,
        "--container-disk-in-gb", "20", "--volume-mount-path", "/data",
        "--ports", "22/tcp,6379/tcp,8080/http,9100/http,8056/http,3001/http,18888/http,8002/http,8099/http",
        "--docker-entrypoint", ",".join(startup_entrypoint()),
    ]
    result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode:
        print(result.stderr, file=sys.stderr, end="")
        return result.returncode
    data = json.loads(result.stdout)
    print("TEMPLATE_ID=" + data["id"])
    return 0

if __name__ == "__main__":
    sys.exit(main())
