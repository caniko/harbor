"""Optional release acceptance using the official MCP SDK, never a model.

Run with the blender-mcp-acceptance output's Python and immutable launchers.
The default checks protocol/disconnected behavior; --interactive adds one
disposable graphical editing workflow. --save-test additionally saves/reopens
a NEW file inside the empty test directory. Upstream suites own broader tests.
"""

import argparse
import asyncio
import base64
import io
import json
import os
from contextlib import asynccontextmanager
from datetime import timedelta
from pathlib import Path

import anyio
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from PIL import Image


def environment(state):
    # Keep compositor connectivity, not credentials or Python/client config.
    env = {
        k: os.environ[k]
        for k in (
            "PATH",
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "XAUTHORITY",
            "DBUS_SESSION_BUS_ADDRESS",
            "LD_LIBRARY_PATH",
            "LIBGL_DRIVERS_PATH",
            "__GLX_VENDOR_LIBRARY_NAME",
            "__EGL_VENDOR_LIBRARY_DIRS",
            "LIBGL_ALWAYS_SOFTWARE",
            "OMP_NUM_THREADS",
        )
        if k in os.environ
    }
    paths = {
        "HOME": "home",
        "XDG_CONFIG_HOME": "config",
        "XDG_DATA_HOME": "data",
        "XDG_CACHE_HOME": "cache",
        "XDG_STATE_HOME": "state",
        "BLENDER_USER_CONFIG": "blender-config",
        "BLENDER_USER_SCRIPTS": "blender-scripts",
        "BLENDER_MCP_RUNTIME_DIR": "runtime",
        "BLENDER_MCP_TMPDIR": "tmp",
    }
    for variable, name in paths.items():
        path = state / name
        path.mkdir(mode=0o700)
        env[variable] = str(path)
    env.update(
        TMPDIR=env["BLENDER_MCP_TMPDIR"], DISABLE_TELEMETRY="true", PYTHONUNBUFFERED="1"
    )
    return env


@asynccontextmanager
async def client(server, env, log):
    async def on_message(message):
        if isinstance(message, Exception):
            raise message

    async with (
        stdio_client(
            StdioServerParameters(command=str(server), env=env), errlog=log
        ) as streams,
        ClientSession(
            *streams,
            read_timeout_seconds=timedelta(seconds=60),
            message_handler=on_message,
        ) as session,
    ):
        with anyio.fail_after(5):
            hello = await session.initialize()
        assert hello.serverInfo.name == "BlenderMCP", hello
        yield session


async def call(session, tool, **arguments):
    with anyio.fail_after(60):
        result = await session.call_tool(
            tool, {"user_prompt": "Canix disposable acceptance", **arguments}
        )
    assert not result.isError, result
    return result


async def data(session, tool, **arguments):
    result = await call(session, tool, **arguments)
    return json.loads(next(item.text for item in result.content if item.type == "text"))


async def code(session, text):
    result = await call(
        session, "execute_blender_code", code=text + "\nprint('CANIX_OK')"
    )
    assert any(
        "CANIX_OK" in item.text for item in result.content if item.type == "text"
    ), result


def socket(node, side, name):
    matches = [
        s["id"]
        for s in node[side]
        if s.get("name") == name or s["id"].split(":", 2)[2] == name
    ]
    assert len(matches) == 1, (name, node[side])
    return matches[0]


async def apply(session, patch, domain="geometry_node"):
    valid = await data(session, f"validate_{domain}_patch", patch=patch)
    assert valid["valid"], valid
    result = await data(session, f"apply_{domain}_patch", patch=patch)
    assert result["status"] == "applied" and result["applied"] and result["mutated"], (
        result
    )
    return result["revision"]


async def edit(session):
    """One graph + material, plus the regression for our serializer patch."""
    sentinel = await data(session, "get_object_info", object_name="Cube")
    created = await data(
        session,
        "create_node_group",
        name="CanixAcceptance",
        tree_type="GeometryNodeTree",
        geometry_is_modifier=True,
    )
    patch = {"schema": "blender-geometry-nodes-patch/1", "tree_name": "CanixAcceptance"}
    rev = await apply(
        session,
        dict(
            patch,
            base_revision=created["revision"],
            operations=[
                {
                    "op": "add_interface_socket",
                    "id": "geo",
                    "name": "Geometry",
                    "in_out": "OUTPUT",
                    "socket_type": "NodeSocketGeometry",
                },
                {
                    "op": "add_node",
                    "id": "out",
                    "name": "Group Output",
                    "node_type": "NodeGroupOutput",
                },
                {
                    "op": "add_node",
                    "id": "cube",
                    "name": "AcceptCube",
                    "node_type": "GeometryNodeMeshCube",
                },
            ],
        ),
    )
    graph = await data(
        session,
        "export_geometry_node_tree",
        tree_name="CanixAcceptance",
        view="operations",
    )
    nodes = graph["tree"]["nodes"]
    size = socket(nodes["AcceptCube"], "inputs", "Size")
    rev = await apply(
        session,
        dict(
            patch,
            base_revision=rev,
            operations=[
                {
                    "op": "add_link",
                    "from_node": "AcceptCube",
                    "to_node": "Group Output",
                    "from_socket": socket(nodes["AcceptCube"], "outputs", "Mesh"),
                    "to_socket": socket(nodes["Group Output"], "inputs", "Geometry"),
                }
            ],
        ),
    )
    # Vector-only mutation: the topology stays unchanged. This assertion
    # failed with the original v1.18.0 extension before to_tuple support.
    vector = dict(
        patch,
        base_revision=rev,
        operations=[
            {
                "op": "set_socket_default",
                "node": "AcceptCube",
                "socket": size,
                "value": [4.0] * 3,
            }
        ],
    )
    changed = await apply(session, vector)
    assert changed != rev, "vector-only edit must change the whole-tree revision"
    stale = await data(session, "apply_geometry_node_patch", patch=vector)
    assert (
        stale["status"] == "rejected" and not stale["applied"] and not stale["mutated"]
    ), stale
    assert [d["code"] for d in stale["diagnostics"]] == ["stale_revision"], stale
    graph = await data(
        session, "export_geometry_node_tree", tree_name="CanixAcceptance", view="all"
    )
    assert graph["revision"] == changed, graph
    defaults = {
        s["id"]: s.get("default")
        for s in graph["tree"]["nodes"]["AcceptCube"]["inputs"]
    }
    assert defaults[size] == [4.0] * 3, defaults

    await data(
        session,
        "ensure_geometry_nodes_modifier",
        object_name="AcceptObj",
        node_group_name="CanixAcceptance",
        create_object_if_missing=True,
        create_modifier_if_missing=True,
    )
    mesh = (await data(session, "inspect_evaluated_mesh", object_name="AcceptObj"))[
        "evaluated"
    ]
    assert (mesh["vertices"], mesh["edges"], mesh["polygons"]) == (8, 12, 6), mesh
    assert mesh["world_bounding_box"] == [[-2.0] * 3, [2.0] * 3], mesh
    after = await data(session, "get_object_info", object_name="Cube")
    assert {k: after[k] for k in ("name", "location", "rotation", "scale")} == {
        k: sentinel[k] for k in ("name", "location", "rotation", "scale")
    }

    # No structured material-creation primitive exists. Graph authoring
    # itself stays on the generic Shader transaction endpoint.
    await code(
        session,
        "import bpy\nm = bpy.data.materials.new('CanixMaterial')\nm.use_nodes = True\n"
        "bpy.data.objects['AcceptObj'].data.materials.append(m)",
    )
    trees = await data(session, "list_node_trees", owner_kinds=["MATERIAL"])
    material = next(t for t in trees["trees"] if t["owner"]["name"] == "CanixMaterial")
    ref = material["tree_ref"]
    shader = await data(
        session,
        "export_node_tree",
        tree_ref=ref,
        view="operations",
        node_names=["Principled BSDF"],
    )
    color = socket(shader["tree"]["nodes"]["Principled BSDF"], "inputs", "Base Color")
    await apply(
        session,
        {
            "schema": "blender-node-tree-patch/1",
            "tree_ref": ref,
            "base_revision": shader["revision"],
            "capabilities": ["graph"],
            "operations": [
                {
                    "op": "set_socket_default",
                    "node": "Principled BSDF",
                    "socket": color,
                    "value": [1.0, 0.0, 0.0, 1.0],
                }
            ],
        },
        "node_tree",
    )
    shader = await data(session, "export_node_tree", tree_ref=ref, view="all")
    colors = {
        s["id"]: s.get("default")
        for s in shader["tree"]["nodes"]["Principled BSDF"]["inputs"]
    }
    assert colors[color] == [1.0, 0.0, 0.0, 1.0], colors


async def interactive(candidate, state, env, save_test, log):
    with (state / "blender.log").open("w") as blender_log:
        blender = await asyncio.create_subprocess_exec(
            str(candidate / "desktop"), env=env, stdout=blender_log, stderr=blender_log
        )
        try:
            async with client(candidate / "server", env, log) as session:
                with anyio.fail_after(60):
                    while True:
                        assert blender.returncode is None, (
                            "Blender exited; inspect blender.log"
                        )
                        records = (
                            await data(
                                session, "list_blender_instances", validate_live=False
                            )
                        )["instances"]
                        targets = [r for r in records if r["pid"] == blender.pid]
                        if targets:
                            break
                        await anyio.sleep(0.2)
                assert len(targets) == 1, targets
                target = targets[0]
                claim = await data(
                    session,
                    "claim_blender_instance",
                    instance_id=target["instance_id"],
                    expected_file_session_id=target["file_session_id"],
                )
                assert claim["active"], claim
                try:
                    await edit(session)
                    shot = await call(session, "get_viewport_screenshot", max_size=512)
                    image = next(c for c in shot.content if c.type == "image")
                    assert image.mimeType == "image/png", image.mimeType
                    raw = base64.b64decode(image.data, validate=True)
                    with Image.open(io.BytesIO(raw)) as picture:
                        picture.load()
                        assert (
                            picture.format == "PNG"
                            and 0 < min(picture.size) <= max(picture.size) <= 512
                        )
                        assert any(
                            lo != hi for lo, hi in picture.convert("RGB").getextrema()
                        ), "blank image"
                    (state / "viewport.png").write_bytes(raw)
                    assert not list(state.rglob("*.blend")), "implicit save"
                    if save_test:
                        destination = state / "acceptance.blend"
                        await code(
                            session,
                            f"import bpy\nbpy.ops.wm.save_as_mainfile(filepath={str(destination)!r})",
                        )
                finally:
                    released = await data(session, "release_blender_instance")
                    assert released["released"], released
        finally:
            if blender.returncode is None:
                blender.terminate()
            try:
                await asyncio.wait_for(blender.wait(), 10)
            except TimeoutError:
                blender.kill()
                await blender.wait()
    assert not list((state / "blender-config").rglob("userpref.blend")), (
        "preferences were saved"
    )
    if save_test:
        # One optional persistence check, on the new file only.
        check = (
            "import bpy\n"
            "tree = bpy.data.node_groups['CanixAcceptance']\n"
            "assert list(tree.nodes['AcceptCube'].inputs['Size'].default_value) == [4.0]*3\n"
            "obj = bpy.data.objects['AcceptObj']\n"
            "material = bpy.data.materials['CanixMaterial']\n"
            "assert obj.data.materials[0] == material\n"
            "assert list(material.node_tree.nodes['Principled BSDF'].inputs['Base Color'].default_value) == [1,0,0,1]\n"
            "print('PERSISTENCE_OK')"
        )
        process = await asyncio.create_subprocess_exec(
            str(candidate / "blender"),
            "--background",
            str(destination),
            "--python-exit-code",
            "1",
            "--python-expr",
            check,
            env=env,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
        )
        try:
            output, _ = await asyncio.wait_for(process.communicate(), 60)
            assert process.returncode == 0 and b"PERSISTENCE_OK" in output, (
                output.decode(errors="replace")
            )
        finally:
            if process.returncode is None:
                process.kill()
                await process.wait()


async def run(args):
    candidate = args.candidate.resolve(strict=True)
    state = args.state_root.resolve()
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    if any(state.iterdir()):
        raise ValueError("state root must be empty")
    # The fixture's configured JSON workspace must remain within the declared
    # acceptance root; no production workspace overrides are used.
    root = Path((candidate / "root").read_text())
    assert state.is_relative_to(root), (state, root)
    report = {
        "candidate": str(candidate),
        "interactive": args.interactive,
        "save_test": args.save_test,
        "executables": {
            n: str((candidate / n).resolve(strict=True))
            for n in ("server", "desktop", "blender", "python")
        },
    }
    env = environment(state)
    try:
        with (state / "mcp.log").open("w") as log:
            async with client(candidate / "server", env, log) as session:
                names = {t.name for t in (await session.list_tools()).tools}
                assert {
                    "list_blender_instances",
                    "claim_blender_instance",
                    "apply_geometry_node_patch",
                    "apply_node_tree_patch",
                    "get_viewport_screenshot",
                } <= names, names
                assert (await data(session, "list_blender_instances"))[
                    "instances"
                ] == []
                disconnected = await session.call_tool(
                    "get_scene_info", {"user_prompt": "disconnected acceptance"}
                )
                assert disconnected.isError and "no_registered_instances" in str(
                    disconnected
                ), disconnected
            if args.interactive:
                await interactive(candidate, state, env, args.save_test, log)
        report["status"] = "passed"
        print("BLENDER_MCP_ACCEPTANCE_OK", state)
    except BaseException as error:
        report.update(status="failed", error=str(error))
        raise
    finally:
        (state / "result.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--state-root", type=Path, required=True)
    parser.add_argument("--interactive", action="store_true")
    parser.add_argument("--save-test", action="store_true")
    args = parser.parse_args()
    if args.save_test and not args.interactive:
        parser.error("--save-test requires --interactive")
    asyncio.run(run(args))
