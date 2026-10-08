"""Verify imported commit ancestry and untouched legacy source trees."""

import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def git(*arguments):
    return subprocess.check_output(["git", *arguments], cwd=ROOT, text=True).strip()


def check_refs():
    receipt = json.loads((ROOT / "migration/refs.json").read_text())
    if receipt["schemaVersion"] != 1:
        raise ValueError("unsupported retained-ref receipt schema")
    heads = receipt["importedHeadNamespace"]
    lines = git(
        "for-each-ref",
        "--format=%(refname)\t%(objectname)",
        heads,
        *receipt["originalTagNamespaces"],
    ).splitlines()
    refs = []
    for line in lines:
        name, object_id = line.split("\t")
        if name.startswith(heads):
            name = receipt["originalHeadNamespace"] + name.removeprefix(heads)
        refs.append(f"{name}\t{object_id}\n")
    digest = hashlib.sha256("".join(sorted(refs)).encode()).hexdigest()
    if len(refs) != receipt["refCount"] or digest != receipt["sourceRefsSha256"]:
        raise ValueError("retained branches/tags differ from their import receipt")
    return len(refs)


def main():
    receipts = json.loads((ROOT / "migration/sources.json").read_text())
    identities = set()
    for receipt in receipts:
        identity = receipt["id"]
        if identity in identities:
            raise ValueError(f"duplicate import identity: {identity}")
        identities.add(identity)
        revision = receipt["sourceRevision"]
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", revision, "HEAD"],
            cwd=ROOT,
            check=True,
        )
        git("cat-file", "-e", f"{receipt['preservedLocalHead']}^{{commit}}")
        if git("rev-parse", f"{revision}^{{tree}}") != receipt["sourceTree"]:
            raise ValueError(f"source tree receipt mismatch: {identity}")
        prefix = receipt["prefix"]
        if prefix.startswith("migration/legacy/"):
            if git("rev-parse", f"HEAD:{prefix}") != receipt["sourceTree"]:
                raise ValueError(f"legacy source tree changed: {identity}")
    refs = check_refs()
    print(
        f"verified {len(identities)} source histories, legacy tree receipts, "
        f"and {refs} retained branches/tags"
    )


if __name__ == "__main__":
    main()
