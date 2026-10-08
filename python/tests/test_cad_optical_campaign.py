"""Original-CAD campaign records include independent lifecycle outcomes."""

import copy
import importlib.util
from pathlib import Path

import pytest


def verifier(monkeypatch):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    spec = importlib.util.spec_from_file_location(
        "cad_optical_campaign", scripts / "verify_cad_spectral_worker.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def report():
    results = [
        {
            "source": name,
            "job": {"state": "succeeded", "exit_code": 0},
            "reimport": {"id": str(index), "state": "queued"},
            "roots_and_reservation_released": True,
        }
        for index, name in enumerate(("origin", "translated"))
    ]
    results.extend(
        {"kind": kind, "job": {"state": state}, "roots_and_reservation_released": True}
        for kind, state in (("cancel", "cancelled"), ("death", "failed"))
    )
    return {"results": results}


def test_two_original_source_records_are_selected_without_rejecting_separate_lifecycle_evidence(
    monkeypatch,
):
    records = verifier(monkeypatch).source_records(report())
    assert [row["source"] for row in records] == ["origin", "translated"]
    # Reimport records were captured before completion; durable private SQLite
    # and registered source checks establish completion before optical planning.
    assert records[0]["reimport"]["state"] == "queued"


def test_unreleased_or_foreign_source_records_cannot_become_optical_prerequisites(
    monkeypatch,
):
    module = verifier(monkeypatch)
    changes = []
    for index in range(4):
        changed = report()
        changed["results"][index]["roots_and_reservation_released"] = False
        changes.append(changed)
    changed = report()
    changed["results"][1]["source"] = "origin"
    changes.append(changed)
    changed = report()
    changed["results"][0]["job"]["state"] = "failed"
    changes.append(changed)
    changed = report()
    changed["results"][0]["job"]["exit_code"] = 1
    changes.append(changed)
    changed = report()
    changed["results"].append(copy.deepcopy(changed["results"][0]))
    changes.append(changed)
    for changed in changes:
        with pytest.raises(ValueError):
            module.source_records(changed)
