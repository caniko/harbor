"""Verify imported commit ancestry and untouched legacy source trees."""

import hashlib
import json
import re
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
    dispositions = (ROOT / "migration/retained-work.md").read_text()
    identities = set()
    retained_commits = set()
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
        heading = f"## {identity}\n"
        if dispositions.count(heading) != 1:
            raise ValueError(f"missing or duplicate disposition section: {identity}")
        section = dispositions.split(heading, 1)[1].split("\n## ", 1)[0]
        documented = re.findall(r"`([a-f0-9]{8})`", section)
        tips = git(
            "for-each-ref",
            "--format=%(objectname)",
            f"refs/tags/imported-ref/{identity}/",
        ).splitlines()
        commits = set(git("rev-list", *tips, f"^{revision}").splitlines())
        expected = {commit[:8] for commit in commits}
        if (
            len(expected) != len(commits)
            or len(documented) != len(set(documented))
            or set(documented) != expected
        ):
            raise ValueError(f"retained-work disposition coverage mismatch: {identity}")
        retained_commits.update(commits)
    refs = check_refs()
    print(
        f"verified {len(identities)} source histories, legacy tree receipts, "
        f"{refs} retained branches/tags, and dispositions for "
        f"{len(retained_commits)} retained commits"
    )


if __name__ == "__main__":
    main()
