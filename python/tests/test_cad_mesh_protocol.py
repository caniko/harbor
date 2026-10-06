"""Imported CAD plans have their own strict schema and preserve source identities."""

import json
import os
import subprocess

import pytest
from jsonschema import Draft202012Validator, ValidationError
from test_cad_mesh import fixture


def test_imported_cad_schema_preserves_world_bounds_without_legacy_or_field_recipe_injection():
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    schemas = json.loads(subprocess.check_output([binary, "schema"]))
    case = json.loads(subprocess.check_output([binary, "case", "init"]))
    geometry = fixture()
    source = {
        "schema_version": 1,
        "job_id": "2fe6dac3-6bcd-439a-bdbd-80d1029ada8c",
        **{
            k: "a" * 64
            for k in (
                "science_id",
                "execution_id",
                "execution_binding_digest",
                "authorization_digest",
            )
        },
        "manifest": {"path": "brep-manifest.json", "sha256": "a" * 64, "bytes": 400},
        "region_evidence": {"path": "regions.json", "sha256": "a" * 64, "bytes": 400},
        "brep": {
            "schema_version": 1,
            "path": "solid.brep",
            "sha256": geometry["brep_sha256"],
            "bytes": geometry["brep_bytes"],
            "format": "brep",
            "units": None,
            "association": None,
            "time_s": None,
            "provenance": "controlled original CAD",
        },
        "geometry": geometry,
    }
    # Use the CLI's required common plan envelope; the Rust behavior tests
    # separately verify source resolution/approval and the exact mesh DAG.
    request = {
        "protocol_version": 1,
        "request_id": "cad-mesh",
        "request": {
            "operation": "plan_cad_mesh",
            "request": {
                "source_job": source["job_id"],
                "region_name": "solid",
                "resolution": 4,
                "geometry_tolerance_m": 1e-6,
            },
        },
    }
    Draft202012Validator(schemas["WorkerRequest"]).validate(request)
    plan = {
        "schema_version": 7,
        "cad_source": source,
        "stages": [],
        "transfers": [],
        "observation": {
            "metrics": ["geometry_correspondence"],
            "probes": [],
            "retained_times_s": [],
            "checkpoint_times_s": [],
            "preview_times_s": [],
            "max_artifact_bytes": 1048576,
            "scientific_congestion": "fail",
            "preview_may_drop": False,
        },
        "policy": "research",
        "fleetix_revision": "a" * 40,
        "fleetix_contract_digest": "a" * 64,
    }
    validator = Draft202012Validator(schemas["ExecutionPlan"])
    validator.validate(plan)
    for changed in [
        {**plan, "cad_source": None},
        {**plan, "case": case},
        {**plan, "fem": None},
        {**plan, "thermal": None},
        *({**plan, "schema_version": v} for v in range(1, 7)),
    ]:
        with pytest.raises(ValidationError):
            validator.validate(changed)
