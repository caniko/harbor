"""Record complete execution of the unchanged 174-method regression corpus."""
import argparse
import contextlib
import hashlib
import json
import os
import pathlib
import stat
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as xml


class Recorded(unittest.TextTestResult):
    def __init__(self, *args):
        super().__init__(*args)
        self.passed = []
        self.executed = []

    def startTest(self, test):
        self.executed.append(test.id())
        super().startTest(test)

    def addSuccess(self, test):
        self.passed.append(test.id())
        super().addSuccess(test)


@contextlib.contextmanager
def store_ownership(fixture):
    """Model Nix's numeric root view without changing production admission."""
    if fixture is None:
        yield
        return
    store = pathlib.Path("/nix/store")
    info = fixture.lstat()
    assert fixture.is_relative_to(store) and stat.S_ISREG(info.st_mode)
    assert not info.st_mode & 0o222, "store root fixture must be immutable"
    root_uid = info.st_uid
    if root_uid == 0:
        yield
        return
    assert root_uid != os.geteuid(), "builder-owned files cannot model store root"
    original = pathlib.Path.lstat

    def mapped(path):
        info = original(path)
        if not path.is_relative_to(store) or info.st_uid != root_uid:
            return info
        fields = list(info)
        fields[4] = 0
        return os.stat_result(fields)

    # Only the explicit immutable store owner's numeric view is translated.
    # Mutable paths, builder ownership and every mode/alias check remain real.
    with patch.object(pathlib.Path, "lstat", mapped):
        yield


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tests", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--case-id", required=True)
    parser.add_argument("--store-root-fixture", type=pathlib.Path)
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.discover(str(args.tests), pattern="test_*.py")
    assert suite.countTestCases() == 174
    with store_ownership(args.store_root_fixture):
        result = unittest.TextTestRunner(verbosity=2, resultclass=Recorded).run(suite)
    document = xml.Element(
        "testsuite", name=args.case_id, tests=str(result.testsRun),
        failures=str(len(result.failures)), errors=str(len(result.errors)),
        skipped=str(len(result.skipped)))
    details = {test.id(): ("failure", text) for test, text in result.failures}
    details.update({test.id(): ("error", text) for test, text in result.errors})
    details.update({test.id(): ("skipped", reason) for test, reason in result.skipped})
    for name in result.executed:
        case = xml.SubElement(document, "testcase", name=name, classname=name.rsplit(".", 1)[0])
        if name in details:
            kind, text = details[name]
            xml.SubElement(case, kind).text = text
    xml.ElementTree(document).write(args.output / "junit.xml", encoding="utf-8", xml_declaration=True)
    acceptance = {
        "schema": 1, "case_id": "case-" + hashlib.sha256(args.case_id.encode()).hexdigest(),
        "assertions": [{"name": name, "passed": name in result.passed} for name in result.executed]}
    (args.output / "python-regressions-acceptance.json").write_text(json.dumps(acceptance) + "\n")
    assert result.testsRun == 174 and len(set(result.executed)) == 174
    assert result.wasSuccessful() and len(result.passed) == 174 and not result.skipped


if __name__ == "__main__":
    main()
