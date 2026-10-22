"""Run a shell command through the standard subprocess interface."""

import subprocess
import sys


result = subprocess.run(
    "echo hello; echo error >&2",
    shell=True,
    capture_output=True,
    text=True,
    check=True,
)
print("stdout:", result.stdout, end="")
print("stderr:", result.stderr, end="")
print("exit:", result.returncode)
sys.stdout.flush()

subprocess.run("echo direct-output", shell=True, check=True)
