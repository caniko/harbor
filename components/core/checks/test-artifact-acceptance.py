import importlib.util
import os
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import tempfile
from threading import Thread
import unittest


spec = importlib.util.spec_from_file_location(
    "acceptance",
    os.environ.get(
        "HARBOR_ACCEPTANCE_MODULE",
        Path(__file__).parent.parent / "lib/artifact-acceptance.py",
    ),
)
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class AcceptanceContract(unittest.TestCase):
    def test_report_requires_every_test_and_rejects_skips_and_duplicates(self):
        for tests in [
            [],
            [{"id": "other", "status": "passed"}],
            [{"id": "review", "status": "skipped"}],
            [{"id": "review", "status": "passed"}] * 2,
            [{"id": "review", "status": "passed"}, {"id": "other", "status": "failed"}],
        ]:
            with self.subTest(tests=tests), self.assertRaises(ValueError):
                acceptance.validate_report({"tests": tests}, ["review"])
        acceptance.validate_report(
            {"tests": [{"id": "review", "status": "passed"}]}, ["review"]
        )

    def test_substituted_asset_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "index.html").write_text("correct")
            identity = acceptance.snapshot({"artifact": directory})
            (root / "index.html").write_text("stale")
            with self.assertRaisesRegex(ValueError, "artifact changed"):
                acceptance.verify_local(identity, root)

    def test_missing_or_extra_assets_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "index.html").write_text("correct")
            identity = acceptance.snapshot({"artifact": directory})
            (root / "stale.wasm").write_bytes(b"stale")
            with self.assertRaises(ValueError):
                acceptance.verify_local(identity, root)
            (root / "stale.wasm").unlink()
            (root / "index.html").unlink()
            with self.assertRaises(ValueError):
                acceptance.verify_local(identity, root)

    def test_empty_artifact_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                acceptance.snapshot({"artifact": directory})

    def test_http_verification_rejects_a_stale_deployed_asset(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "app").mkdir()
            (root / "index.html").write_text("landing")
            (root / "app/bundle.wasm").write_bytes(b"correct")
            identity = acceptance.snapshot({"artifact": directory})
            server = ThreadingHTTPServer(
                ("127.0.0.1", 0), partial(SimpleHTTPRequestHandler, directory=directory)
            )
            thread = Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                url = f"http://127.0.0.1:{server.server_port}"
                acceptance.verify_http(identity, url, "app/")
                (root / "app/bundle.wasm").write_bytes(b"stale")
                with self.assertRaisesRegex(ValueError, "served asset differs"):
                    acceptance.verify_http(identity, url, "app/")
            finally:
                server.shutdown()
                server.server_close()
                thread.join()

    def test_mismatched_backend_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "index.html").write_text("correct")
            identity = acceptance.snapshot(
                {"artifact": directory, "backend": directory}
            )
            with self.assertRaisesRegex(ValueError, "backend does not match"):
                acceptance.verify_local(identity, root, root / "old-backend")


if __name__ == "__main__":
    unittest.main()
