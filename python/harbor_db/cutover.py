"""Mandatory read-only cutover admission; explicit filesystem custody certification."""

import argparse
import json
import os
import pwd
import re
import signal
import stat
import subprocess
import sys
import time
from pathlib import Path

from . import postgres, recovery, resource
from .durable import read_json, write_json


def validate_manifest(value, host):
    if (not isinstance(value, dict) or value.get("version") != 1 or value.get("enforced") is not True or value.get("host") != host
            or not isinstance(value.get("resources"), dict)
            or not re.fullmatch(r"[A-Za-z0-9_-]+", host)):
        raise ValueError("unsupported cutover contract or target host mismatch")
    for key, default, maximum in (("timeout_seconds", 30, 300), ("activation_timeout_seconds", 900, 3600)):
        timeout = value.get(key, default)
        if type(timeout) is not int or not 1 <= timeout <= maximum:
            raise ValueError(f"invalid bounded cutover inspection budget: {key}")
    for name, entry in value["resources"].items():
        if (not re.fullmatch(r"[A-Za-z0-9_-]+", name) or not isinstance(entry, dict)
                or entry.get("kind") not in ("postgres", "filesystem")
                or not isinstance(entry.get("user"), str)
                or not re.fullmatch(r"[A-Za-z0-9_-]+", entry["user"])):
            raise ValueError(f"invalid cutover resource: {name}")
        if entry["kind"] == "postgres":
            validate_path(entry.get("config"))
            if entry["user"] != "postgres":
                raise ValueError("PostgreSQL admission requires the postgres service identity")
        else:
            authority = entry.get("authority", {})
            directories = authority.get("directories")
            if (not isinstance(directories, list) or not directories
                    or len(set(directories)) != len(directories)
                    or authority.get("resource") != name or not isinstance(authority.get("binding"), dict)
                    or type(entry.get("max_age_seconds")) is not int or entry["max_age_seconds"] <= 0):
                raise ValueError(f"invalid existing filesystem authority: {name}")
            validate_path(authority.get("state_dir"))
            validate_path(entry.get("custody_file"))
            if Path(entry["custody_file"]).parent != Path(authority["state_dir"]):
                raise ValueError("custody receipt must be directly in its private authority directory")
            for directory in directories:
                validate_path(directory)
            units = entry.get("runtime_units")
            if not isinstance(units, list) or any(
                not isinstance(unit, str) or not re.fullmatch(r"[A-Za-z0-9_.@:-]+\.service", unit) for unit in units
            ):
                raise ValueError("filesystem custody requires explicit writer service units")
            dependency = entry.get("database_resource")
            if dependency is not None and value["resources"].get(dependency, {}).get("kind") != "postgres":
                raise ValueError("filesystem custody requires an enrolled PostgreSQL recovery dependency")
    return value


def validate_path(path):
    if not isinstance(path, str) or not Path(path).is_absolute() or os.path.normpath(path) != path:
        raise ValueError("cutover storage and policy paths must be canonical absolute paths")


def digest(path):
    return recovery.digest(path)


def failed_walk(error):
    raise error


def root_identities(config):
    return [{"device": Path(path).stat().st_dev, "inode": Path(path).stat().st_ino}
            for path in config["authority"]["directories"]]


def inventory(config, *, contents):
    """Metadata is an early rejection filter; activation always hashes the corpus."""
    result = []
    authority = config["authority"]
    for directory in authority["directories"]:
        root = Path(directory)
        if not root.is_dir() or str(root.resolve()) != str(root):
            raise ValueError(f"authoritative source is missing or redirected: {root}")
        files = {}
        marker = resource.anchor(authority, directory)
        for parent, directories, names in os.walk(root, followlinks=False, onerror=failed_walk):
            for name in directories + names:
                path = Path(parent) / name
                if path == marker:
                    continue
                info = path.lstat()
                if not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)):
                    raise ValueError(f"redirected or special corpus entry: {path}")
                key = str(path.relative_to(root))
                files[key] = {
                    "directory": stat.S_ISDIR(info.st_mode),
                    "mode": stat.S_IMODE(info.st_mode),
                    "size": info.st_size if stat.S_ISREG(info.st_mode) else 0,
                }
                if stat.S_ISREG(info.st_mode):
                    if contents:
                        files[key]["sha256"] = digest(path)
                        after = path.lstat()
                        if any(getattr(after, key) != getattr(info, key) for key in
                               ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")):
                            raise ValueError(f"corpus entry changed during inspection: {path}")
                    else:
                        files[key].update(mtime_ns=info.st_mtime_ns, ctime_ns=info.st_ctime_ns,
                                          inode=info.st_ino, device=info.st_dev)
        if not any(not item["directory"] for item in files.values()):
            raise ValueError(f"authoritative corpus is empty: {root}")
        result.append(files)
    return result


def writer_active(unit):
    result = subprocess.run(
        ["systemctl", "show", unit, "--property=ActiveState", "--value"],
        check=True, capture_output=True, text=True, timeout=5,
    )
    return result.stdout.strip() not in ("inactive", "failed")


def require_database_paths(inventories, requirements):
    """A matching restore cannot hide repositories referenced by database rows."""
    for requirement in requirements:
        path = requirement["path"]
        root = requirement["root"]
        if (not isinstance(path, str) or not path or Path(path).is_absolute()
                or ".." in Path(path).parts or os.path.normpath(path) != path
                or type(root) is not int or not 0 <= root < len(inventories)
                or type(requirement.get("directory")) is not bool):
            raise ValueError("invalid database-bound relative corpus path")
        entry = inventories[root].get(path)
        if entry is None or entry["directory"] != requirement["directory"] or (not entry["directory"] and entry["size"] == 0):
            raise ValueError(f"database references missing, empty or incompatible corpus entry: {path}")


def certify_filesystem(config, restored_roots, identifier, *, now=None, database_snapshot=None, database_requirements=()):
    """Writers must already be stopped; this command never stops or restores them."""
    now = int(time.time()) if now is None else now
    authority = config["authority"]
    resource.contract(authority)
    roots = list(map(Path, authority["directories"]))
    restores = list(map(Path, restored_roots))
    state = resource.state_directory(authority)
    if len(restores) != len(roots) or any(
        not root.is_absolute() or root.resolve() != root
        or any(root == source or root.is_relative_to(source) or source.is_relative_to(root)
               for source in roots + [state]) for root in restores
    ):
        raise ValueError("restore roots must be independent absolute corpus directories")
    if config.get("database_resource") and not database_snapshot:
        raise ValueError("filesystem certification needs matching database recovery evidence")
    with resource.adoption(authority, identifier) as publish:
        if any(writer_active(unit) for unit in config["runtime_units"]):
            raise ValueError("source writer is active; establish the declared consistency window")
        identities = root_identities(config)
        source = inventory(config, contents=True)
        require_database_paths(source, database_requirements)
        restored = inventory({"authority": {**authority, "directories": restored_roots}}, contents=True)
        if source != restored:
            raise ValueError("source and restored historical corpus differ")
        for original, restored_root, files in zip(roots, restores, source, strict=True):
            if any(not item["directory"] and (original / name).samefile(restored_root / name)
                   for name, item in files.items()):
                raise ValueError("restore corpus aliases a source file; require an independent restore")
        metadata = inventory(config, contents=False)
        if inventory(config, contents=True) != source:
            raise ValueError("source changed during custody certification")
        if any(writer_active(unit) for unit in config["runtime_units"]) or root_identities(config) != identities:
            raise ValueError("source writer or storage identity changed during custody certification")
        publish()
        write_json(recovery.absolute(config["custody_file"]), {
            "version": 1, "resource": authority["resource"], "identity": identifier,
            "binding": authority["binding"], "directories": authority["directories"],
            "completed_at": now, "database_snapshot_sha256": database_snapshot,
            "database_requirements": database_requirements,
            "inventory": source, "metadata": metadata, "root_identities": identities,
        })


def check_resource(config, *, phase, now=None):
    if phase not in ("preflight", "activate", "startup", "certify"):
        raise ValueError("unsupported cutover phase")
    now = int(time.time()) if now is None else now
    if config["kind"] == "filesystem":
        # Missing/empty roots are rejected before authority or receipt inspection.
        metadata = inventory(config, contents=False)
        authority = config["authority"]
        with resource.inspection(authority) as adopted:
            receipt = read_json(recovery.absolute(config["custody_file"]))
            if (receipt.get("version") != 1 or receipt.get("resource") != authority["resource"]
                    or receipt.get("identity") != adopted["identity"]
                    or receipt.get("binding") != authority["binding"]
                    or receipt.get("directories") != authority["directories"]
                    or receipt.get("root_identities") != root_identities(config)):
                raise ValueError("filesystem custody binding differs from the adopted authority")
            if phase == "startup":
                return {}
            recovery.fresh(receipt.get("completed_at"), now, config["max_age_seconds"])
            if metadata != receipt.get("metadata"):
                raise ValueError("source corpus changed; repeat custody certification")
            if phase != "preflight" and inventory(config, contents=True) != receipt.get("inventory"):
                raise ValueError("source corpus contents differ from the certified restore")
            return {"database_snapshot_sha256": receipt.get("database_snapshot_sha256"),
                    "database_requirements": receipt.get("database_requirements", [])}
    else:
        database = read_json(config["config"])
        postgres.check(database)
        if phase == "startup":
            return {}
        settings = recovery.policy(database)
        postgres.reject_upgrade(database)
        if phase != "startup":
            postgres.inspect_live(database, settings["system_identifier"],
                                  config.get("socket_dir", "/run/postgresql"), config.get("port", 5432))
            for check in config.get("compatibility_checks", []):
                observed = recovery.query(database, config.get("socket_dir", "/run/postgresql"),
                                          config.get("port", 5432), check["database"], check["sql"])
                if observed.strip() != "t":
                    raise ValueError(f"candidate schema/recovery compatibility check failed: {check['database']}")
        with recovery.admission(database, now=now, verify_contents=phase in ("activate", "certify")):
            if phase == "certify" and recovery.records(database, settings, config.get("socket_dir", "/run/postgresql"),
                                                       config.get("port", 5432)) != read_json(settings["snapshot_file"])["records"]:
                raise ValueError("live database records differ from the recovery snapshot")
            requirements = {}
            for name, checks in config.get("corpus_checks", {}).items():
                requirements[name] = []
                for check in checks:
                    paths = json.loads(recovery.query(database, config.get("socket_dir", "/run/postgresql"),
                                                     config.get("port", 5432), check["database"], check["sql"]))
                    if not isinstance(paths, list):
                        raise TypeError("database corpus query must return a JSON array of paths")
                    requirements[name].extend({"root": check["root"], **item} for item in paths)
            return {"database_snapshot_sha256": digest(settings["snapshot_file"]), "corpus_requirements": requirements}


def execute_worker(path, name, config, phase, timeout, extra=(), *, command_name="check"):
    account = pwd.getpwnam(config["user"])
    kwargs = {}
    if os.geteuid() == 0:
        kwargs = {"user": account.pw_uid, "group": account.pw_gid,
                  "extra_groups": os.getgrouplist(account.pw_name, account.pw_gid)}
    elif os.geteuid() != account.pw_uid:
        raise ValueError(f"inspection requires the resource's service user: {account.pw_name}")
    command = [sys.executable, "-B", "-m", "harbor_db.cutover", command_name, "--contract", str(path),
               "--host", read_json(path)["host"], "--phase", phase, "--worker", name, *extra]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               text=True, start_new_session=True, **kwargs)
    try:
        output, error = process.communicate(timeout=timeout)
    except BaseException:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        raise
    if process.returncode:
        raise ValueError(error.strip() or "resource inspection failed")
    return json.loads(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    checking = commands.add_parser("check", help="read-only mandatory rebuild admission")
    certifying = commands.add_parser("certify", help="explicit custody proof from an independent restore")
    certification_worker = commands.add_parser("certify-worker", help=argparse.SUPPRESS)
    serving = commands.add_parser("serve", help="exec a declared writer retaining its resource authority lease")
    serving.add_argument("--resource", required=True)
    serving.add_argument("argv", nargs=argparse.REMAINDER)
    for command in (checking, certifying, certification_worker, serving):
        command.add_argument("--contract", type=Path, required=True)
        command.add_argument("--host", required=True)
    checking.add_argument("--phase", choices=("preflight", "activate", "startup", "certify"), default="preflight")
    checking.add_argument("--worker", help=argparse.SUPPRESS)
    certification_worker.add_argument("--phase", choices=("certify",), required=True)
    certification_worker.add_argument("--worker", required=True)
    certification_worker.add_argument("--certify-roots", nargs="+", required=True)
    certification_worker.add_argument("--certify-identity", required=True)
    certification_worker.add_argument("--database-snapshot")
    certification_worker.add_argument("--database-requirements", type=json.loads, default=[])
    certifying.add_argument("--resource", required=True)
    certifying.add_argument("--restore-root", action="append", required=True)
    certifying.add_argument("--identity", required=True)
    args = parser.parse_args()
    try:
        manifest = validate_manifest(read_json(args.contract.resolve()), args.host)
        if args.command == "serve":
            config = manifest["resources"][args.resource]
            argv = args.argv[1:] if args.argv[:1] == ["--"] else args.argv
            resource.serve(config["authority"], argv, inspect=lambda: check_resource(config, phase="startup"))
        elif args.command == "certify":
            config = manifest["resources"][args.resource]
            dependency = config.get("database_resource")
            database = None
            requirements = []
            if dependency:
                result = execute_worker(args.contract, dependency, manifest["resources"][dependency],
                                        "certify", manifest.get("activation_timeout_seconds", 900))
                database = result["database_snapshot_sha256"]
                requirements = result.get("corpus_requirements", {}).get(args.resource, [])
            extra = ["--certify-identity", args.identity, "--certify-roots", *args.restore_root]
            if database:
                extra = ["--database-snapshot", database, "--database-requirements", json.dumps(requirements), *extra]
            execute_worker(args.contract, args.resource, config, "certify",
                           manifest.get("activation_timeout_seconds", 900), extra, command_name="certify-worker")
        elif args.command == "certify-worker":
            certify_filesystem(manifest["resources"][args.worker], args.certify_roots,
                                args.certify_identity, database_snapshot=args.database_snapshot,
                                database_requirements=args.database_requirements)
            print("{}")
        elif args.worker:
            print(json.dumps(check_resource(manifest["resources"][args.worker], phase=args.phase)))
        else:
            failures = []
            results = {}
            deadline = time.monotonic() + manifest.get(
                "timeout_seconds" if args.phase == "preflight" else "activation_timeout_seconds", 30 if args.phase == "preflight" else 900)
            for name, config in manifest["resources"].items():
                try:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise ValueError("cutover inspection budget exhausted")
                    results[name] = execute_worker(args.contract, name, config, args.phase, remaining)
                except (ValueError, OSError, subprocess.TimeoutExpired) as error:
                    failures.append({"resource": name, "reason": str(error)})
            for name, config in manifest["resources"].items():
                dependency = config.get("database_resource")
                if args.phase != "startup" and dependency and (dependency not in results or name not in results
                                    or results[name].get("database_snapshot_sha256") != results[dependency].get("database_snapshot_sha256")
                                    or results[name].get("database_requirements", []) != results[dependency].get("corpus_requirements", {}).get(name, [])):
                    failures.append({"resource": name, "reason": "database recovery snapshot differs from filesystem custody"})
            print(json.dumps({"version": 1, "host": args.host, "phase": args.phase,
                              "status": "blocked" if failures else "ready", "failures": failures}))
            return int(bool(failures))
    except (RuntimeError, OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(f"harbor-db-cutover: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
