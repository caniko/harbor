"""Rust-owned native transfer requests reject substitution and weakened bounds."""

import json
import os
import subprocess

import pytest
from jsonschema import Draft202012Validator, ValidationError


def fixture():
    return {
        "schema_version": 1,
        "source_job": "00000000-0000-0000-0000-000000000001",
        "physical_time_s": 120.0,
        "destination": {
            "region": "lower",
            "size_m": [0.02, 0.01, 0.01],
            "origin_m": [0.0, 0.0, 0.0],
        },
        "maximum_projection_error_k": 1.0,
        "maximum_relative_conservation_error": 1e-12,
    }


def test_projection_schema_and_worker_operation_reject_caller_temperatures(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    schemas = json.loads(subprocess.check_output([binary, "schema"]))
    validator = Draft202012Validator(schemas["ThermalProjectionRequest"])
    validator.validate(fixture())
    for key, value in (("temperature_k", 293.15), ("mesh", {}), ("density", 1000.0)):
        with pytest.raises(ValidationError):
            validator.validate({**fixture(), key: value})
    operation = {"operation": "results_transfer_temperature", "request": fixture()}
    Draft202012Validator(schemas["WorkerRequest"]).validate(
        {"protocol_version": 1, "request_id": "projection", "request": operation}
    )
    path = tmp_path / "projection.json"
    path.write_text(json.dumps(fixture()))
    reply = subprocess.run(
        [
            binary,
            "--socket",
            str(tmp_path / "absent.sock"),
            "results",
            "transfer-temperature",
            str(path),
        ],
        capture_output=True,
        check=False,
    )
    assert reply.returncode != 0 and json.loads(reply.stdout)["ok"] is False
