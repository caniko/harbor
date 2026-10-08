"""Indexed native history requests preserve the standalone schema and units."""

import json
import os
import subprocess

import pytest
from jsonschema import Draft202012Validator, ValidationError


def test_coupled_history_and_moisture_requests_have_bounded_typed_stage_selectors():
    schemas = json.loads(
        subprocess.check_output([os.environ["HARBOR_CAD_TEST_BINARY"], "schema"])
    )
    sample = {
        "schema_version": 1,
        "job_id": "00000000-0000-0000-0000-000000000001",
        "field": "temperature",
        "physical_time_s": 120.0,
        "locations": [{"association": "node", "node_id": 1}],
    }
    moisture = {
        "schema_version": 1,
        "job_id": sample["job_id"],
        "physical_time_s": 120.0,
        "surface_region": "xmin",
        "moisture_risk": {"assessment": "missing", "reason": "Humidity not supplied"},
    }
    for name, request in (
        ("ThermalSampleRequest", sample),
        ("NativeMoistureRequest", moisture),
    ):
        validator = Draft202012Validator(schemas[name])
        validator.validate(request)
        for index in (0, 1):
            validator.validate({**request, "thermal_stage": index})
        for index in ("../thermal-upper", -1, 0.5, [0], True):
            with pytest.raises(ValidationError):
                validator.validate({**request, "thermal_stage": index})
        with pytest.raises(ValidationError):
            validator.validate(
                {**request, "artifact_path": "stages/foreign/reference.dat"}
            )
