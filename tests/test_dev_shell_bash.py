"""Check the Bash selected by a dev shell's PATH without host startup files."""

import argparse
import os
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--path", default=os.environ["PATH"])
    args = parser.parse_args()
    bash = shutil.which("bash", path=args.path)
    if bash is None:
        raise RuntimeError("dev shell has no Bash on PATH")
    env = {
        "PATH": args.path,
        "TERM": "dumb",
        "HISTFILE": "/dev/null",
        "PS1": r"\[\e[32m\]harbor-bash-test\[\e[0m\] > ",
    }
    subprocess.run(
        [bash, "--noprofile", "--norc", "-c", "shopt -s progcomp"],
        env=env, check=True,
    )
    result = subprocess.run(
        [bash, "--noprofile", "--norc", "-ic", 'printf "%s" "${PS1@P}"'],
        env=env, check=True, text=True, capture_output=True,
    )
    # Readline represents nonprinting spans with SOH/STX in prompt expansion.
    rendered = result.stdout.replace("\x01", "").replace("\x02", "")
    expected = "\x1b[32mharbor-bash-test\x1b[0m > "
    if rendered != expected:
        raise RuntimeError(f"Bash prompt rendered incorrectly: {result.stdout!r}")
    print(f"Interactive Bash completion and prompt rendering passed: {bash}")


if __name__ == "__main__":
    main()
