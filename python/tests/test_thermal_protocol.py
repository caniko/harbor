"""Version-6 physical histories remain independent of prior recipe schemas."""

import json
import os
import subprocess

import pytest
from jsonschema import Draft202012Validator, ValidationError
from test_thermal_history import fixture


def test_thermal_cli_schema_and_digest_preserve_the_full_prescribed_history(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    spec = fixture()
    path = tmp_path / "thermal.json"
    path.write_text(json.dumps(spec))
    reply = json.loads(
        subprocess.check_output([binary, "case", "plan-thermal-reference", str(path)])
    )
    plan = reply["plan"]
    assert plan["schema_version"] == 6 and plan["thermal"] == spec
    assert plan["observation"]["retained_times_s"] == spec["observation_times_s"]
    assert "case" not in plan and "fem" not in plan
    schema = json.loads(subprocess.check_output([binary, "schema"]))
    validator = Draft202012Validator(schema["ExecutionPlan"])
    validator.validate(plan)
    for changed in [
        {**plan, "thermal": None},
        {**plan, "fem": None},
        {**plan, "case": None},
        *({**plan, "schema_version": v} for v in range(1, 6)),
    ]:
        with pytest.raises(ValidationError):
            validator.validate(changed)
    spec["heater_history"][-1][1] = 2.0
    path.write_text(json.dumps(spec))
    changed = json.loads(
        subprocess.check_output([binary, "case", "plan-thermal-reference", str(path)])
    )
    assert changed["approval_digest"] != reply["approval_digest"]
    validator.validate(changed["plan"])
