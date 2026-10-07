"""Development campaign subprocesses must keep their isolated virtualenv ABI."""

import importlib.util
import json
import subprocess
import sys
from pathlib import Path


def test_development_mcp_keeps_virtualenv_interpreter_and_imports_sdk(monkeypatch):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    loader = importlib.util.spec_from_file_location(
        "retention_campaign", scripts / "verify_wetting_retention.py"
    )
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    invoked = Path(sys.executable).absolute()
    command = module.mcp_executable(invoked, development=True)
    result = json.loads(
        subprocess.check_output(
            [
                str(command),
                "-c",
                "import json,sys;from mcp import Client;print(json.dumps({'prefix':sys.prefix,'sdk':Client.__name__}))",
            ],
            env={"PATH": "/nonexistent", "PYTHONDONTWRITEBYTECODE": "1"},
        )
    )
    assert result == {"prefix": sys.prefix, "sdk": "Client"}
    assert command == invoked
