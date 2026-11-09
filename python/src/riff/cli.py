"""Expose the Riff command through Python's module and script entry points."""

import sys

from riff._native import cli


def main() -> int:
    """Write complete command output to its intended streams and return its exit code."""
    stdout, stderr, exit_code = cli(sys.argv[1:])
    sys.stdout.write(stdout)
    sys.stderr.write(stderr)
    return exit_code
