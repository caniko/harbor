"""Read-only recovery admission and executed, record-level acceptance receipts.

Snapshots require a consumer-owned consistency window. Certification compares a
disposable read-only restored endpoint with that snapshot; it never restores,
promotes, adopts or changes a database. Remote certification uses the same
copied backup, snapshot and query contract and records its actual executor host.
"""

import contextlib
import hashlib
import json
import os
import re
import socket
import time
from pathlib import Path

from . import postgres
from .durable import lock, read_json, write_json


def absolute(path):
    path = Path(path)
    if not path.is_absolute() or str(path.resolve()) != str(path):
        raise ValueError(f"recovery path must be absolute and not redirected: {path}")
    return path


def digest(path):
    # read_json and all artifact reads reject the final symlink as well as
    # redirected ancestors. Do not silently certify a different backup tree.
    path = absolute(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fresh(timestamp, now, max_age):
    if type(timestamp) is not int or not 0 <= now - timestamp <= max_age:
        raise ValueError("recovery evidence timestamp is stale, future or invalid")


def policy(config):
    settings = config["recovery"]
    paths = [settings["snapshot_file"], settings["receipt_file"]]
    if settings.get("off_host_receipt_file"):
        paths.append(settings["off_host_receipt_file"])
    # A certifier on the independent host may select off_host_receipt_file as
    # its receipt output, but neither output may overwrite the source snapshot.
    if settings["snapshot_file"] in paths[1:]:
        raise ValueError("recovery receipts must not overwrite the record snapshot")
    if not re.fullmatch(r"[1-9][0-9]*", settings["system_identifier"]):
        raise ValueError("recovery requires an independently recorded system identifier")
    if type(settings["max_age_seconds"]) is not int or settings["max_age_seconds"] <= 0:
        raise ValueError("recovery evidence age must be positive")
    checks = settings["record_checks"]
    if not checks or len({item["name"] for item in checks}) != len(checks):
        raise ValueError("recovery requires uniquely named record checks")
    for item in checks:
        if any(not isinstance(item[key], str) or not item[key].strip() or "\0" in item[key]
               for key in ("name", "database", "sql")):
            raise ValueError("invalid recovery record check")
    return settings


def contract(settings):
    return hashlib.sha256(json.dumps(settings["record_checks"], sort_keys=True).encode()).hexdigest()


def lsn(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9A-Fa-f]{1,8}/[0-9A-Fa-f]{1,8}", value):
        raise ValueError("invalid recovery LSN")
    high, low = value.split("/")
    return (int(high, 16) << 32) + int(low, 16)


def backup(config, settings, now):
    root = absolute(settings["backup_root"])
    marker = absolute(root / "LAST_SUCCESS")
    fd = os.open(marker, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd) as stream:
        identifier = stream.read(256).strip()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", identifier) or identifier.endswith(".partial"):
        raise ValueError("invalid completed backup identifier")
    directory = absolute(root / "base" / identifier)
    manifest = absolute(directory / "backup_manifest")
    meta = read_json(absolute(root / "base" / f"{identifier}.meta.json"))
    if (meta["backup_id"] != identifier or str(meta["pg_major"]) != str(config["major"])
            or meta["system_identifier"] != settings["system_identifier"]):
        raise ValueError("backup identity does not match the declared primary")
    if lsn(meta["post_backup_lsn"]) <= lsn(meta["backup_stop_lsn"]):
        raise ValueError("backup recovery point must follow its stop LSN")
    fresh(int(manifest.stat().st_mtime), now, settings["max_age_seconds"])
    if postgres.inspect_cluster(config["package"], directory, config["major"]) != settings["system_identifier"]:
        raise ValueError("backup control-file identity differs from the primary")
    return directory, {
        "backup_id": identifier, "system_identifier": settings["system_identifier"],
        "major": str(config["major"]), "epoch_id": meta["epoch_id"],
        "manifest_sha256": digest(manifest), "recovery_target_lsn": meta["post_backup_lsn"],
    }


def verify_backup(config, directory):
    postgres.run([Path(config["package"]) / "bin/pg_verifybackup", "--no-parse-wal", directory],
                 capture_output=True, text=True, timeout=config["recovery"]["verify_timeout_seconds"])


def query(config, socket_dir, port, database, sql):
    if not Path(socket_dir).is_absolute() or "," in socket_dir or not 1 <= port <= 65535:
        raise ValueError("recovery queries require a local Unix socket and valid port")
    env = {key: value for key, value in os.environ.items() if not key.startswith("PG")}
    env["PGCONNECT_TIMEOUT"] = "5"
    # Consumer SQL is declarative trusted configuration, never interpolated
    # user data. Run every check in an explicit read-only transaction.
    return postgres.run([
        Path(config["package"]) / "bin/psql", "--no-psqlrc", "--no-password", "--quiet",
        f"--host={socket_dir}", f"--port={port}", "--username=postgres", f"--dbname={database}",
        "--set=ON_ERROR_STOP=1", "--tuples-only", "--no-align",
        "--command", f"BEGIN READ ONLY;\n{sql}\n;COMMIT;",
    ], env=env, capture_output=True, text=True, timeout=60).stdout


def records(config, settings, socket_dir, port):
    return {item["name"]: hashlib.sha256(query(config, socket_dir, port, item["database"], item["sql"]).encode()).hexdigest()
            for item in settings["record_checks"]}


def evidence(path, label, binding, settings, now):
    try:
        value = read_json(absolute(path))
    except FileNotFoundError as error:
        raise ValueError(f"missing {label}; execute record snapshot and restore certification") from error
    if value.get("version") != 1 or any(value.get(key) != expected for key, expected in binding.items()):
        raise ValueError(f"{label} is for a different backup")
    if value.get("record_contract_sha256") != contract(settings):
        raise ValueError(f"{label} record-check contract differs")
    fresh(value.get("completed_at"), now, settings["max_age_seconds"])
    return value


@contextlib.contextmanager
def evidence_lease(settings, *, inspect=False):
    # Backup mutation and evidence publication have distinct locks. A drill may
    # retain the backup's shared lease while certifying; it cannot race another
    # source snapshot or certifier into publishing mixed evidence.
    anchor = absolute(settings["snapshot_file"]).parent / "recovery.lock"
    with lock(anchor, shared=inspect, create=not inspect):
        yield


def snapshot(config, socket_dir, port, *, now=None):
    settings = policy(config)
    now = int(time.time()) if now is None else now
    with lock(absolute(settings["backup_root"]) / "locks/mutate", shared=True), evidence_lease(settings):
        directory, binding = backup(config, settings, now)
        postgres.inspect_live(config, settings["system_identifier"], socket_dir, port)
        verify_backup(config, directory)
        result = {"version": 1, **binding, "completed_at": now,
                  "record_contract_sha256": contract(settings),
                  "records": records(config, settings, socket_dir, port)}
        write_json(absolute(settings["snapshot_file"]), result)
    return result


def inspect_restored(config, socket_dir, port):
    return json.loads(query(config, socket_dir, port, "postgres", """
        SELECT json_build_object(
            'data_dir', current_setting('data_directory'),
            'major', (current_setting('server_version_num')::int / 10000)::text,
            'system_identifier', system_identifier::text,
            'read_only', current_setting('default_transaction_read_only'),
            'in_recovery', pg_is_in_recovery(),
            'replay_lsn', pg_last_wal_replay_lsn()::text)
        FROM pg_control_system()
    """))


def certify(config, data_dir, socket_dir, port, *, now=None, hostname=None):
    settings = policy(config)
    now = int(time.time()) if now is None else now
    restored = absolute(data_dir)
    if restored == absolute(config["data_dir"]):
        raise ValueError("cannot certify the authoritative primary as a restored copy")
    with lock(absolute(settings["backup_root"]) / "locks/mutate", shared=True), evidence_lease(settings):
        directory, binding = backup(config, settings, now)
        source = evidence(settings["snapshot_file"], "record snapshot", binding, settings, now)
        verify_backup(config, directory)
        observed = inspect_restored(config, socket_dir, port)
        expected = {"data_dir": str(restored), "major": str(config["major"]),
                    "system_identifier": settings["system_identifier"], "read_only": "on", "in_recovery": False}
        if any(observed.get(key) != value for key, value in expected.items()):
            raise ValueError("restored endpoint is not the declared disposable read-only recovery")
        if lsn(observed["replay_lsn"]) < lsn(binding["recovery_target_lsn"]):
            raise ValueError("restored endpoint did not reach the recovery point")
        actual = records(config, settings, socket_dir, port)
        if actual != source.get("records"):
            raise ValueError("restored application records differ from the source snapshot")
        result = {"version": 1, "status": "ready", **binding, "completed_at": now,
                  "record_contract_sha256": contract(settings), "records": actual,
                  "snapshot_sha256": digest(settings["snapshot_file"]),
                  "executor_host": socket.gethostname() if hostname is None else hostname,
                  "restored_data_dir": str(restored), "replay_lsn": observed["replay_lsn"]}
        write_json(absolute(settings["receipt_file"]), result)
    return result


def check(config, *, now=None):
    settings = policy(config)
    now = int(time.time()) if now is None else now
    # Inspection never creates or replaces the persistent backup lock anchor.
    with lock(absolute(settings["backup_root"]) / "locks/mutate", shared=True):
        directory, binding = backup(config, settings, now)
        evidence(settings["snapshot_file"], "record snapshot", binding, settings, now)
        with evidence_lease(settings, inspect=True):
            return check_evidence(config, settings, directory, binding, now)


def check_evidence(config, settings, directory, binding, now):
    source = evidence(settings["snapshot_file"], "record snapshot", binding, settings, now)
    expected_names = {item["name"] for item in settings["record_checks"]}
    if set(source.get("records", {})) != expected_names or any(
        not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value)
        for value in source["records"].values()
    ):
        raise ValueError("record snapshot is incomplete")
    verify_backup(config, directory)
    result = {"status": "ready", **binding, "off_host": None}
    paths = [(settings["receipt_file"], "restore acceptance")]
    if settings.get("off_host_receipt_file"):
        paths.append((settings["off_host_receipt_file"], "off-host restore acceptance"))
    for path, label in paths:
        receipt = evidence(path, label, binding, settings, now)
        if (receipt.get("status") != "ready" or receipt.get("records") != source["records"]
                or receipt.get("snapshot_sha256") != digest(settings["snapshot_file"])
                or receipt.get("restored_data_dir") == config["data_dir"]
                or not receipt.get("restored_data_dir")
                or receipt["completed_at"] < source["completed_at"]
                or lsn(receipt.get("replay_lsn")) < lsn(binding["recovery_target_lsn"])):
            raise ValueError(f"{label} is not complete record-level recovery evidence")
        if label.startswith("off-host"):
            if not receipt.get("executor_host") or receipt["executor_host"] == settings["source_hostname"]:
                raise ValueError("off-host recovery must execute on an independent host")
            result["off_host"] = receipt["executor_host"]
    return result
