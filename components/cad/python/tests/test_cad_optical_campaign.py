"""Original-CAD campaign records include independent lifecycle outcomes."""

import copy
import fcntl
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


def test_closed_original_snapshot_preserves_stale_socket_and_every_database_byte(
    monkeypatch, tmp_path
):
    import socket

    module = verifier(monkeypatch)
    source = tmp_path / "source"
    source.mkdir()
    (source / "worker.lock").write_bytes(b"")
    (source / "artifacts").mkdir()
    (source / "artifacts/original.json").write_bytes(b'{"original":true}')
    for name in ("jobs.sqlite3", "jobs.sqlite3-wal", "jobs.sqlite3-shm"):
        (source / name).write_bytes(("preserved-" + name).encode())
    endpoint = source / "worker.sock"
    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(str(endpoint))
    before = {p.name: p.read_bytes() for p in source.iterdir() if p.is_file()}
    destination = tmp_path / "copied"
    destination.mkdir()
    module.copy_closed_source_state(source, destination)
    assert endpoint.exists() and not (destination / "worker.sock").exists()
    assert {p.name: p.read_bytes() for p in source.iterdir() if p.is_file()} == before
    for name in ("jobs.sqlite3", "jobs.sqlite3-wal", "jobs.sqlite3-shm"):
        assert (destination / name).read_bytes() == before[name]
        assert (destination / name).stat().st_ino != (source / name).stat().st_ino
    rejected = tmp_path / "rejected"
    rejected.mkdir()
    with (source / "worker.lock").open("rb") as active:
        fcntl.flock(active.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        with pytest.raises(ValueError, match="closed original source worker"):
            module.copy_closed_source_state(source, rejected)
    assert not list(rejected.iterdir())


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
