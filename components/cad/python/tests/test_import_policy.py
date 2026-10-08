import importlib.util
from pathlib import Path

import pytest


def policy():
    path = Path(__file__).parents[2] / "adapters/import_policy.py"
    spec = importlib.util.spec_from_file_location("import_policy", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize(
    "damage",
    [
        None,
        "writable_input",
        "writable_plan",
        "broad_store",
        "extra_store",
        "writable_package",
        "missing_package",
    ],
)
def test_importer_requires_only_declared_read_only_package_mounts(damage):
    module = policy()
    package = "/nix/store/00000000000000000000000000000000-importer"
    mounts = {
        package: {"ro"},
        "/plan.json": {"ro"},
        "/input.FCStd": {"ro"},
        "/work": {"rw"},
        "/import-runtime-closure.txt": {"ro"},
    }
    if damage == "writable_input":
        mounts["/input.FCStd"] = {"rw"}
    elif damage == "writable_plan":
        mounts["/plan.json"] = {"rw"}
    elif damage == "broad_store":
        mounts["/nix/store"] = {"ro"}
    elif damage == "extra_store":
        mounts["/nix/store/11111111111111111111111111111111-foreign"] = {"ro"}
    elif damage == "writable_package":
        mounts[package] = {"rw"}
    elif damage == "missing_package":
        del mounts[package]
    if damage is None:
        module.verify_mounts(mounts, {package}, "cad_inspect")
    else:
        with pytest.raises(RuntimeError):
            module.verify_mounts(mounts, {package}, "cad_inspect")


@pytest.mark.parametrize(
    "no_new_privs,capabilities", [("0", "0000000000000000"), ("1", "0000000000000001")]
)
def test_importer_rejects_effective_privilege_drift(no_new_privs, capabilities):
    with pytest.raises(RuntimeError):
        policy().verify_privileges({"NoNewPrivs": no_new_privs, "CapEff": capabilities})
