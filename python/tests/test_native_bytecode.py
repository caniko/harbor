"""Dynamic immutable helper imports cannot add bytecode to a closure-only namespace."""

import subprocess
import sys
from pathlib import Path

import pytest


@pytest.mark.parametrize("adapter,operation", [("thermal_history", "run")])
def test_native_helper_import_preserves_exact_store_root_before_request_rejection(
    tmp_path, adapter, operation
):
    store = tmp_path / "store"
    store.mkdir()
    helper = store / "immutable-fem-helper.py"
    helper.write_text(
        "def read_regular(path,maximum):\n    return b'{}'\ndef strict_json(raw):\n    return {}\n"
    )
    original = Path(__file__).resolve().parents[2] / f"adapters/{adapter}.py"
    source = original.read_text().replace("@fem_bridge@", str(helper))
    code = "import sys\nscope={'__name__':'adapter_test'}\nexec(compile(sys.stdin.read(),'native_adapter','exec'),scope)\ntry:\n    scope['main']()\nexcept ValueError as error:\n    print(error)\nelse:\n    raise AssertionError('invalid request should fail before native output')\n"
    result = subprocess.run(
        [sys.executable, "-c", code, operation, str(tmp_path / "request.json")],
        input=source,
        text=True,
        capture_output=True,
        check=True,
    )
    assert "required" in result.stdout
    assert list(store.iterdir()) == [helper]
