"""Exercise the generated NixOS preactivation hook without production privileges."""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    hook = Path(sys.argv[1]).read_text()
    runner, systemctl = sys.argv[2:4]
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        calls = root / "calls.jsonl"
        incoming = root / "incoming.json"
        run = root / "systemd-run"
        stop = root / "systemctl"
        body = (
            f"#!{sys.executable}\n"
            "import json, os, signal, sys\n"
            "with open(os.environ['CALLS'], 'a') as stream:\n"
            "    stream.write(json.dumps([os.path.basename(sys.argv[0]), *sys.argv[1:]]) + '\\n')\n"
            "if os.path.basename(sys.argv[0]) == 'systemd-run':\n"
            "    if os.environ['RESULT'] == 'interrupt':\n"
            "        os.kill(os.getppid(), signal.SIGTERM)\n"
            "    sys.exit(23 if os.environ['RESULT'] == 'fail' else 0)\n"
        )
        for executable in (run, stop):
            executable.write_text(body)
            executable.chmod(0o700)
        script = root / "pre-switch"
        script.write_text(hook.replace(runner, str(run)).replace(systemctl, str(stop))
                          .replace("/srv/import/off-host.json", str(incoming))
                          + "\nprintf 'admission-next\\n'\n")
        count = 0
        for action in ("switch", "test", "boot", "dry-activate", "check"):
            for supplied in (False, True):
                if supplied:
                    incoming.write_text("{}")
                else:
                    incoming.unlink(missing_ok=True)
                for outcome in ("ready", "fail", "interrupt"):
                    calls.unlink(missing_ok=True)
                    result = subprocess.run(
                        ["bash", str(script), "/candidate", action],
                        env={**os.environ, "CALLS": str(calls), "RESULT": outcome},
                        capture_output=True, text=True, timeout=10, check=False,
                    )
                    events = [json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []
                    preparing = action in ("switch", "test")
                    expected = ({"ready": 0, "fail": 23, "interrupt": 143}[outcome] if preparing else 0)
                    assert result.returncode == expected, (action, outcome, result.stderr)
                    assert ("admission-next" in result.stdout) == (expected == 0)
                    if preparing:
                        argv = events[0][1:]
                        assert events[0][0] == "systemd-run"
                        assert "--wait" in argv and "--collect" in argv
                        assert "--property=User=postgres" in argv and "--property=Group=postgres" in argv
                        assert "--property=ReadWritePaths=/srv/backups /srv/disposable" in argv
                        assert "prepare-recovery" in argv and "--preparation-config" in argv
                        assert not any(value in argv for value in ("adopt", "adopt-live", "initdb"))
                        assert any("LoadCredential=recovery-off-host:" in value for value in argv) == supplied
                        assert [event[0] for event in events] == (["systemd-run", "systemctl"] if expected else ["systemd-run"])
                        if expected:
                            assert events[1][1] == "stop"
                            assert events[1][2] == next(value.removeprefix("--unit=") for value in argv if value.startswith("--unit=")) + ".service"
                    else:
                        assert events == []
                    count += 1
        print(f"Managed recovery preparation: {count} action/credential/failure/cancellation cases passed")


if __name__ == "__main__":
    main()
