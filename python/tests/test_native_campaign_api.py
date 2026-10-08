"""Qualification entrypoints must call the real shared lifecycle API/signatures."""

import ast
import importlib.util
import inspect
from pathlib import Path


def test_native_worker_campaigns_use_existing_shared_methods_and_valid_signatures(
    monkeypatch,
):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    spec = importlib.util.spec_from_file_location(
        "native_worker_campaign", scripts / "native_worker_campaign.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    for name in (
        "verify_spectral_worker.py",
        "verify_freezing_worker.py",
        "verify_atmosphere_worker.py",
        "verify_atmospheric_transport_worker.py",
        "verify_snow_worker.py",
        "verify_study_worker.py",
    ):
        tree = ast.parse((scripts / name).read_text())
        for node in ast.walk(tree):
            if (
                not isinstance(node, ast.Call)
                or not isinstance(node.func, ast.Attribute)
                or not isinstance(node.func.value, ast.Name)
                or node.func.value.id != "campaign"
            ):
                continue
            method = getattr(module.WorkerCampaign, node.func.attr, None)
            assert callable(method), (
                f"{name}:{node.lineno} calls missing shared lifecycle method {node.func.attr}"
            )
            # Each argument is supplied by the entrypoint at runtime; binding
            # proves its arity/keyword contract without manufacturing native work.
            inspect.signature(method).bind(
                None,
                *([None] * len(node.args)),
                **{k.arg: None for k in node.keywords if k.arg is not None},
            )


def test_native_campaigns_call_registered_mcp_tool_names():
    repo = Path(__file__).parents[2]
    server = ast.parse((repo / "python/harbor_cad_mcp/server.py").read_text())
    registered = {
        node.name
        for node in ast.walk(server)
        if isinstance(node, ast.AsyncFunctionDef)
        and any(
            isinstance(decorator, ast.Call)
            and isinstance(decorator.func, ast.Attribute)
            and decorator.func.attr == "tool"
            for decorator in node.decorator_list
        )
    }
    for filename in (
        "verify_snow_worker.py",
        "verify_atmospheric_transport_worker.py",
        "verify_atmosphere_worker.py",
        "verify_spectral_worker.py",
        "verify_study_worker.py",
    ):
        for node in ast.walk(ast.parse((repo / "scripts" / filename).read_text())):
            if not isinstance(node, ast.Call):
                continue
            tool = None
            if isinstance(node.func, ast.Name) and node.func.id == "mcp_call":
                tool = node.args[1]
            elif isinstance(node.func, ast.Attribute) and node.func.attr == "call_tool":
                tool = node.args[0]
            if isinstance(tool, ast.Constant) and isinstance(tool.value, str):
                assert tool.value in registered, (
                    f"{filename}:{node.lineno}: unregistered MCP tool {tool.value}"
                )
