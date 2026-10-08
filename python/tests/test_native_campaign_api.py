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
