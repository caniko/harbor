"""Shared exact-package worker lifecycle operations for native qualification."""

import asyncio
import json
import os
import socket
import subprocess
import time

from verify_systemd import wait_job


class WorkerCampaign:
    def __init__(self, binary, mcp, runtime, authority, root, timeout=300):
        self.binary, self.mcp = binary, mcp
        self.authority = authority.resolve(strict=True)
        self.root, self.state = root, root / "state"
        self.endpoint, self.profile = self.state / "worker.sock", root / "profile.json"
        self.timeout = timeout
        self.environment = {
            key: os.environ[key]
            for key in ("HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "PATH")
            if key in os.environ
        }
        self.profile.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "policy": "research",
                    "allowed_input_root": str(root),
                    "max_ram_bytes": 2 * 1024**3,
                    "max_disk_bytes": 2 * 1024**3,
                    "threads": 2,
                    "timeout_seconds": timeout,
                    "native_runtime": str(runtime),
                    "service_mode": "systemd",
                }
            )
        )
        self.worker, self.log, self.owned = None, None, []

    def __enter__(self):
        self.log = (self.root / "worker.log").open("xb")
        try:
            self.start()
        except BaseException:
            self.__exit__(None, None, None)
            raise
        return self

    def __exit__(self, *_):
        for unit in self.owned:
            subprocess.run(
                ["systemctl", "--user", "stop", unit],
                capture_output=True,
                check=False,
                timeout=30,
            )
        if self.worker is not None and self.worker.poll() is None:
            self.worker.terminate()
            self.worker.wait(timeout=5)
        if self.log is not None:
            self.log.close()

    def start(self):
        self.worker = subprocess.Popen(
            [
                str(self.binary),
                "worker",
                "--state",
                str(self.state),
                "--profile",
                str(self.profile),
                "--authority",
                str(self.authority),
            ],
            env=self.environment,
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if self.worker.poll() is not None:
                raise RuntimeError("worker startup failed; inspect retained worker.log")
            try:
                with socket.socket(socket.AF_UNIX) as connection:
                    connection.settimeout(1)
                    connection.connect(str(self.endpoint))
                    connection.sendall(
                        b'{"protocol_version":1,"request_id":"ready","request":{"operation":"doctor"}}\n'
                    )
                    if json.loads(connection.recv(65536))["ok"]:
                        return
            except OSError:
                pass
            time.sleep(0.02)
        raise TimeoutError("exact worker startup readiness")

    def restart(self):
        self.worker.kill()
        self.worker.wait(timeout=5)
        self.start()

    def command(self, *argv, allow_error=False):
        process = subprocess.run(
            [str(self.binary), *map(str, argv)],
            env=self.environment,
            capture_output=True,
            timeout=30,
            check=False,
        )
        reply = json.loads(process.stdout)
        if not allow_error and process.returncode:
            raise RuntimeError(reply)
        return reply

    def planned(self, operation, spec, key):
        path = self.root / f"spec-{key}.json"
        path.write_text(json.dumps(spec, allow_nan=False))
        return self.command("case", operation, path)

    def submit(self, plan, key, approval=None, allow_error=False):
        path = self.root / f"plan-{key}.json"
        path.write_text(json.dumps(plan["plan"], allow_nan=False))
        reply = self.command(
            "--socket",
            self.endpoint,
            "job",
            "submit",
            path,
            "--approve",
            approval or plan["approval_digest"],
            "--idempotency-key",
            key,
            allow_error=allow_error,
        )
        if allow_error:
            return reply
        job = reply["data"]
        if job["unit"] not in self.owned:
            self.owned.append(job["unit"])
        return job

    def mcp_submit(self, operation, spec, plan, key):
        async def invoke():
            from mcp import Client
            from mcp.client.stdio import StdioServerParameters

            parameters = StdioServerParameters(
                command=str(self.mcp),
                args=["--profile", "simulation"],
                env={**self.environment, "HARBOR_CAD_SOCKET": str(self.endpoint)},
            )
            async with Client(parameters) as client:
                planned = await client.call_tool(operation, {"spec": spec})
                if planned.is_error or planned.structured_content != plan:
                    raise ValueError("exact CLI/MCP immutable approval parity required")
                submitted = await client.call_tool(
                    "job_submit",
                    {
                        "plan": plan["plan"],
                        "approved_digest": plan["approval_digest"],
                        "idempotency_key": key,
                    },
                )
                if submitted.is_error:
                    raise RuntimeError(submitted.content)
                return submitted.structured_content

        job = asyncio.run(invoke())
        self.owned.append(job["unit"])
        return job

    def wait(self, job, states):
        return wait_job(
            str(self.binary),
            self.endpoint,
            job["id"],
            set(states),
            timeout=self.timeout + 10,
        )
