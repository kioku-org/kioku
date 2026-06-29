import os
from pathlib import Path
from fastapi import FastAPI, Request, Header, HTTPException, Response

app = FastAPI()

DATA_DIR = Path(os.environ.get("DATA_DIR", "/data"))
TOKEN = os.environ.get("COOKIE_SERVICE_TOKEN", "").strip()


def check_auth(authorization: str = "") -> None:
    if TOKEN and authorization != f"Bearer {TOKEN}":
        raise HTTPException(status_code=401, detail="Unauthorized")


@app.get("/health")
def health():
    return {"status": "ok"}


@app.get("/userdata/{user_id}")
async def get_userdata(user_id: str, authorization: str = Header(default="")):
    check_auth(authorization)
    path = DATA_DIR / f"{user_id}.tar.gz"
    if not path.exists():
        raise HTTPException(status_code=404, detail="Not found")
    return Response(content=path.read_bytes(), media_type="application/octet-stream")


@app.put("/userdata/{user_id}")
async def put_userdata(user_id: str, request: Request, authorization: str = Header(default="")):
    check_auth(authorization)
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    path = DATA_DIR / f"{user_id}.tar.gz"
    path.write_bytes(await request.body())
    return {"message": "stored"}


@app.delete("/userdata/{user_id}")
async def delete_userdata(user_id: str, authorization: str = Header(default="")):
    check_auth(authorization)
    path = DATA_DIR / f"{user_id}.tar.gz"
    if not path.exists():
        raise HTTPException(status_code=404, detail="Not found")
    path.unlink()
    return {"message": "deleted"}
