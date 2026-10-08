"""Versioned, bounded Unix protocol shared with the CLI; arrays remain artifacts."""

import asyncio
import json
import os
import uuid
from pathlib import Path

MAX_MESSAGE = 65536


class WorkerError(ValueError):
    """An anticipated typed rejection returned by the local Rust worker."""


async def request(operation: str, **payload: object) -> dict:
    path = os.environ.get("HARBOR_CAD_SOCKET")
    if not path:
        raise ValueError("HARBOR_CAD_SOCKET must identify the local Rust worker")
    if not Path(path).is_absolute():
        raise ValueError("worker socket must be absolute")
    request_id = str(uuid.uuid4())
    message = (
        json.dumps(
            {
                "protocol_version": 1,
                "request_id": request_id,
                "request": {"operation": operation, **payload},
            },
            allow_nan=False,
        ).encode()
        + b"\n"
    )
    if len(message) > MAX_MESSAGE:
        raise ValueError("request exceeds 64 KiB protocol limit")
    reader, writer = await asyncio.wait_for(
        asyncio.open_unix_connection(path, limit=MAX_MESSAGE), 5
    )
    try:
        writer.write(message)
        await writer.drain()
        line = await asyncio.wait_for(reader.readline(), 15)
        if len(line) > MAX_MESSAGE or not line.endswith(b"\n"):
            raise ValueError("invalid bounded worker response")
        response = json.loads(line)
        if (
            response.get("protocol_version") != 1
            or response.get("request_id") != request_id
        ):
            raise ValueError("worker response identity/version mismatch")
        if not response.get("ok"):
            error = response.get("error", {})
            raise WorkerError(
                f"{error.get('code', 'worker_error')}: {error.get('message', '')}"
            )
        return response["data"]
    finally:
        writer.close()
        await writer.wait_closed()
