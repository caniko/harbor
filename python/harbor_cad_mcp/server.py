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

        @server.tool()
        async def case_plan_openlb_reference(case: dict[str, Any]) -> dict[str, Any]:
            """Plan explicit native CPU OpenLB with FreeCAD geometry; systemd policy required."""
            return await request("plan_openlb_reference", case=case)

        @server.tool()
        async def case_plan_b1(
            case: dict[str, Any], selections: dict[str, Any]
        ) -> dict[str, Any]:
            """Plan required HIP or CUDA flow, independent EGL/VAAPI devices, and offline bundle.

            Device selectors are explicit requests; planning does not qualify hardware.
            """
            return await request("plan_b1", case=case, selections=selections)

    if profile in {"cad", "all"}:

        @server.tool()
        async def cad_plan_mesh(request_spec: dict[str, Any]) -> dict[str, Any]:
            """Plan a CPU mesh from one registered authorized CAD solid; preserve exact BREP/world placement."""
            return await request("plan_cad_mesh", request=request_spec)

        @server.tool()
        async def cad_mesh_submit(
            plan: dict[str, Any], approved_digest: str, idempotency_key: str
        ) -> dict[str, Any]:
            """Submit only an approved source-bound mesh; return its durable job ID."""
            if plan.get("schema_version") != 7 or [
                s.get("operation") for s in plan.get("stages", [])
            ] != ["cad_mesh", "bundle"]:
                raise ValueError("exact approved imported CAD mesh DAG required")
            return await request(
                "submit",
                plan=plan,
                approved_digest=approved_digest,
                idempotency_key=idempotency_key,
            )

        @server.tool()
        async def cad_plan_inspection(
            case: dict[str, Any], max_artifact_bytes: int = 67108864
        ) -> dict[str, Any]:
            """Plan digest-approved source inspection in FreeCAD's sandbox; no solver.

            Source paths are relative to the worker's allowed root. Approval and
            ordinary job submission are required before any document is opened.
            """
            return await request(
                "plan_cad_inspection", case=case, max_artifact_bytes=max_artifact_bytes
            )

        @server.tool()
        async def cad_submit(
            plan: dict[str, Any], approved_digest: str, idempotency_key: str
        ) -> dict[str, Any]:
            """Submit only an approved CAD inspection plan; return its durable job ID."""
            operations = [stage.get("operation") for stage in plan.get("stages", [])]
            if plan.get("schema_version") != 1 or operations != [
                "cad_inspect",
                "bundle",
            ]:
                raise ValueError("exact approved CAD inspection DAG required")
            return await request(
                "submit",
                plan=plan,
                approved_digest=approved_digest,
                idempotency_key=idempotency_key,
            )

    if profile in {"cad", "results", "all"}:

        @server.tool()
        async def cad_regions(job_id: str) -> dict[str, Any]:
            """Read bounded checksummed named-solid metadata from one native CAD job."""
            return await request("cad_regions", job_id=job_id)

    if profile in {"simulation", "all"}:

        @server.tool()
        async def case_plan_fem_reference(spec: dict[str, Any]) -> dict[str, Any]:
            """Plan an explicit static synthetic CPU FEM job; no native imports or solve."""
            return await request("plan_fem_reference", spec=spec)

        @server.tool()
        async def fem_plan_imported(request_spec: dict[str, Any]) -> dict[str, Any]:
            """Plan a synthetic CPU static FEM reference from registered authorized world-space CAD."""
            return await request("plan_fem_imported", request=request_spec)

        @server.tool()
        async def case_plan_thermal_reference(spec: dict[str, Any]) -> dict[str, Any]:
            """Plan explicit synthetic transient CPU heat transfer; prescribed convection and physical histories."""
            return await request("plan_thermal_reference", spec=spec)

        @server.tool()
        async def filter_plan(request_spec: dict[str, Any]) -> dict[str, Any]:
            """Plan required HIP point-gradient filtering from one registered retained time.

            A numerical compute stage with independent approval; no solver or rendering.
            """
            return await request("plan_filter", request=request_spec)

        @server.tool()
        async def cold_restart_validate(case: dict[str, Any]) -> dict[str, Any]:
            """Validate cold-restart histories and material ranges, preserving missing inputs.

            Returns prescribed heater energy and bounded moisture screening only;
            does not launch a transient solver or infer boot reliability.
            """
            return await request("validate_cold_restart", case=case)

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

    if profile in {"cad", "simulation", "results", "all"}:

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
        async def render_plan(request_spec: dict[str, Any]) -> dict[str, Any]:
            """Plan retained-field presentation with exact source binding and independent devices."""
            return await request("plan_presentation", request=request_spec)

        @server.tool()
        async def video_plan(request_spec: dict[str, Any]) -> dict[str, Any]:
            """Plan independent hardware encoding from registered rendered frames."""
            return await request("plan_video", request=request_spec)

        @server.tool()
        async def presentation_submit(
            plan: dict[str, Any], approved_digest: str, idempotency_key: str
        ) -> dict[str, Any]:
            """Submit only an approved source-bound presentation; return a durable job ID."""
            if plan.get("schema_version") not in {2, 3} or not isinstance(
                plan.get("source"), dict
            ):
                raise ValueError("source-bound presentation plan required")
            return await request(
                "submit",
                plan=plan,
                approved_digest=approved_digest,
                idempotency_key=idempotency_key,
            )

        @server.tool()
        async def results_describe(job_id: str) -> dict[str, Any]:
            """Return hashes/provenance and artifact descriptors, never scientific arrays."""
            return await request("describe", job_id=job_id)

        @server.tool()
        async def qualification_report(job_id: str) -> dict[str, Any]:
            """Inspect historical job evidence bound to its exact source/runtime/device; never promote qualification."""
            return await request("qualification_report", job_id=job_id)

        @server.tool()
        async def artifact_list(
            job_id: str, after: str | None = None, limit: int = 20
        ) -> dict[str, Any]:
            """Read a byte-bounded page; pass next_after until null to retain every descriptor.

            For a stable full traversal, wait for a terminal job state. Export always copies all records.
            """
            if not 1 <= limit <= 100:
                raise ValueError("artifact page limit 1..100 required")
            return await request("artifacts", job_id=job_id, after=after, limit=limit)

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
