"""Version-5 FEM identity and absence of invented fluid/time fields across schemas."""

import json
import os
import subprocess

import pytest
from jsonschema import Draft202012Validator, ValidationError


def test_fem_cli_plan_and_schema_preserve_the_independent_si_recipe(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    spec = {
        "schema_version": 1,
        "synthetic": True,
        "backend": "cpu",
        "mode": "thermal_boundary",
        "size_m": [0.02, 0.01, 0.01],
        "resolution": 4,
        "geometry_tolerance_m": 1e-6,
        "temperatures_k": [293.15, 303.15],
        "numerical_tolerance": 1e-6,
        "conductivity_w_m_k": 20.0,
    }
    source = tmp_path / "fem.json"
    source.write_text(json.dumps(spec))
    reply = json.loads(
        subprocess.check_output([binary, "case", "plan-fem-reference", str(source)])
    )
    plan = reply["plan"]
    assert plan["schema_version"] == 5 and plan["fem"] == spec and "case" not in plan
    assert plan["observation"]["retained_times_s"] == []
    schema = json.loads(subprocess.check_output([binary, "schema"]))
    validator = Draft202012Validator(schema["ExecutionPlan"])
    validator.validate(plan)
    for changed in (
        {**plan, "case": None},
        {**plan, "fem": None},
        {**plan, "schema_version": 4},
    ):
        with pytest.raises(ValidationError):
            validator.validate(changed)
    spec["mode"] = "free_expansion"
    del spec["conductivity_w_m_k"]
    spec.update(young_modulus_pa=200e9, poisson_ratio=0.3, expansion_per_k=12e-6)
    source.write_text(json.dumps(spec))
    changed = json.loads(
        subprocess.check_output([binary, "case", "plan-fem-reference", str(source)])
    )
    assert changed["approval_digest"] != reply["approval_digest"]
    validator.validate(changed["plan"])
