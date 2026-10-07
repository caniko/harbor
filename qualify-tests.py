"""Retain complete mandatory lifecycle results from the built Nix test output."""
import json
from pathlib import Path
import sys
import unittest


def main():
    suite = unittest.defaultTestLoader.discover(sys.argv[1], pattern="test_*.py")
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    output = Path(sys.argv[2])
    output.mkdir()
    receipt = {"schema": "harbor-db-mandatory-tests.v1", "qualified": False,
               "cases": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
               "skips": len(result.skipped), "expected_failures": len(result.expectedFailures),
               "unexpected_successes": len(result.unexpectedSuccesses), "retries": 0}
    (output / "tests.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if not result.wasSuccessful() or result.testsRun < 175 or result.skipped or result.expectedFailures:
        raise SystemExit("Mandatory lifecycle roster requires at least 175 cases and zero failures, errors, skips, or retries")


if __name__ == "__main__":
    main()
