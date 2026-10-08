"""Native refusal evidence uses the real CLI's structured stdout contract."""

import importlib.util
import json
import os
import subprocess
from pathlib import Path

import pytest
from test_spectral_native import fixture


def test_native_applicability_campaign_accepts_typed_cli_stdout_and_rejects_false_refusals(
    monkeypatch, tmp_path
):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    loader = importlib.util.spec_from_file_location(
        "spectral_campaign", scripts / "verify_spectral_cpu.py"
    )
    campaign = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(campaign)
    spec = fixture()
    spec["precision"] = "Float64"
    request = tmp_path / "request.json"
    request.write_text(json.dumps(spec))
    work = tmp_path / "native"
    work.mkdir()
    cli = subprocess.run(
        [
            os.environ["HARBOR_CAD_TEST_BINARY"],
            "case",
            "validate-spectral-reference",
            str(request),
        ],
        capture_output=True,
        check=False,
    )
    assert cli.returncode == 1
    assert json.loads(cli.stdout)["error"]["code"] == "invalid_input"
    native = subprocess.CompletedProcess([], 1, b"", b"ValueError: rejected input\n")
    campaign.verify_rejection(native, cli, work)
    for changed in (
        subprocess.CompletedProcess([], 0, cli.stdout, b""),
        subprocess.CompletedProcess([], 1, b'{"ok":true}', b"invalid_input"),
        subprocess.CompletedProcess(
            [], 1, b'{"ok":false,"error":{"code":"unqualified"}}', b"invalid_input"
        ),
        subprocess.CompletedProcess([], 1, b"not JSON", b"invalid_input"),
    ):
        with pytest.raises(ValueError):
            campaign.verify_rejection(native, changed, work)
    (work / "unexpected.exr").write_bytes(b"partial")
    with pytest.raises(ValueError):
        campaign.verify_rejection(native, cli, work)
