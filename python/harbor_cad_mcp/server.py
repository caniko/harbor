"""Official SDK v2.3 MCPServer API, verified against tag source and a real client.

Source: https://github.com/modelcontextprotocol/python-sdk/tree/2118f14f8a19bc158d8a1cf90af58d85d187f849
"""

import argparse
from typing import Any

from mcp.server import MCPServer
from .client import request


def build_server(profile: str) -> MCPServer:
    if profile not in {"cad", "simulation", "results", "all"}:
        raise ValueError("unknown MCP profile")
    server = MCPServer(f"harbor-cad-{profile}")

    @server.tool()
    async def doctor() -> dict[str, Any]:
        """Inspect read-only inventory and independent qualification states."""
        return await request("doctor")

    @server.tool()
    async def backend_list() -> list:
        """List model/backend qualification; no native imports or GPU launch."""
        return await request("backend_list")

    if profile in {"cad", "simulation", "all"}:

        @server.tool()
        async def case_validate(case: dict[str, Any]) -> dict[str, Any]:
            """Validate explicit units, named regions and applicability."""
            return await request("validate", case=case)

        @server.tool()
        async def case_plan(case: dict[str, Any]) -> dict[str, Any]:
            """Prepare a CPU analytical-reference plan; never solve implicitly."""
            return await request("plan", case=case)

    if profile in {"simulation", "all"}:

        @server.tool()
        async def job_submit(
            plan: dict[str, Any], approved_digest: str, idempotency_key: str
        ) -> dict[str, Any]:
            """Submit an immutable approved plan and immediately return a durable job ID."""
            return await request(
                "submit",
                plan=plan,
                approved_digest=approved_digest,
                idempotency_key=idempotency_key,
            )

        @server.tool()
        async def job_cancel(job_id: str) -> dict[str, Any]:
            """Cancel only a tracked owned service tree, with bounded escalation."""
            return await request("cancel", job_id=job_id)

    if profile in {"simulation", "results", "all"}:

        @server.tool()
        async def job_status(job_id: str) -> dict[str, Any]:
            """Read durable state; process success does not imply physical validation."""
            return await request("status", job_id=job_id)

        @server.tool()
        async def job_logs(job_id: str, after: int = 0, limit: int = 20) -> list:
            """Read at most 100 bounded structured events using a durable cursor."""
            if after < 0 or not 1 <= limit <= 100:
                raise ValueError("bounded nonnegative cursor and 1..100 limit required")
            return await request("logs", job_id=job_id, after=after, limit=limit)

    if profile in {"results", "all"}:

        @server.tool()
        async def results_describe(job_id: str) -> dict[str, Any]:
            """Return hashes/provenance and artifact descriptors, never scientific arrays."""
            return await request("describe", job_id=job_id)

        @server.tool()
        async def artifact_list(job_id: str) -> list:
            """List retained scientific records and relative paths for offline export."""
            return await request("artifacts", job_id=job_id)

    return server


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--profile", choices=["cad", "simulation", "results", "all"], default="all"
    )
    args = parser.parse_args()
    build_server(args.profile).run(transport="stdio")


if __name__ == "__main__":
    main()
