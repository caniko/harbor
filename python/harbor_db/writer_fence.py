"""Explicit, durable application-writer exclusion for a PostgreSQL cutover.

Root and the local PostgreSQL OS control identity remain trusted. An HBA fence
excludes application SQL roles, including SQL superusers, and allows only
declared authenticated *physical* replication connections. Acquisition requires
a restart: hba_file can only be set at server start, not by configuration reload.

https://www.postgresql.org/docs/18/runtime-config-file-locations.html#GUC-HBA-FILE
https://www.postgresql.org/docs/18/auth-pg-hba-conf.html
"""

import hashlib
import ipaddress
import re
from pathlib import Path


def policy(config):
    settings = config.get("writer_fence")
    if not isinstance(settings, dict):
        raise TypeError("explicit writer fence policy is required")
    if (not re.fullmatch(r"[1-9][0-9]*", settings["system_identifier"])
            or type(settings["required_for_recovery"]) is not bool
            or not Path(settings["normal_hba_file"]).is_absolute()):
        raise ValueError("invalid writer fence identity or normal HBA")
    for library in settings["allowed_preload_libraries"]:
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]*", library):
            raise ValueError("invalid allowed preload library")
    for peer in settings["replication_peers"]:
        role = peer["role"]
        if (not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,62}", role)
                or role in ("all", "postgres", "replication", "sameuser", "samerole")):
            raise ValueError("invalid physical replication role")
        ipaddress.ip_network(peer["address"], strict=True)
    return settings


def hba(config):
    settings = policy(config)
    lines = ["local all postgres peer", "local replication postgres peer"]
    lines += [f"host replication {peer['role']} {peer['address']} scram-sha-256"
              for peer in settings["replication_peers"]]
    lines += ["local all all reject", "local replication all reject",
              "host all all 0.0.0.0/0 reject", "host all all ::/0 reject",
              "host replication all 0.0.0.0/0 reject", "host replication all ::/0 reject"]
    return ("\n".join(lines) + "\n").encode()


def binding(config):
    settings = policy(config)
    # A newer package, normal HBA or retired enrollment cannot silently discard
    # an active exclusion epoch for the same authoritative cluster and policy.
    value = {key: config[key] for key in ("resource", "major", "data_dir", "state_dir")}
    value.update(system_identifier=settings["system_identifier"],
                 allowed_preload_libraries=sorted(settings["allowed_preload_libraries"]),
                 hba_sha256=hashlib.sha256(hba(config)).hexdigest())
    return value


def paths(config):
    state = Path(config["state_dir"])
    if not state.is_absolute() or state.resolve() != state:
        raise ValueError("writer fence state must be canonical")
    return {"journal": state / "writer-fence.json", "lock": state / "writer-fence.lock",
            "hba": state / "writer-fence.hba"}
