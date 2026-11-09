"""Compare the two bindings using one contract and a portable custom provider."""

import json
import shutil
import subprocess
import sys
from pathlib import Path

from riff import FixtureFactory, JSONValue, ProviderContext
from riff.contracts import case_json

ROOT = Path(__file__).resolve().parents[2]


def account_id(context: ProviderContext) -> JSONValue:
    """Use every seed bit without language-specific Faker behavior."""
    return f"acct_{context.seed:016x}"


def main() -> None:
    """Require identical batches and negative-case diagnostics from both bindings."""
    factory = FixtureFactory.from_file(
        ROOT / "conformance/schema.json", providers={"synthetic.id": account_id}
    )
    passes = factory.generate(count=20, seed=9223372036854775825, profile="boundary")
    failures = factory.failure_cases(count=20, seed=9223372036854775825, profile="boundary")
    expected: dict[str, JSONValue] = {
        "passes": passes,
        "failures": [case_json(case) for case in failures],
    }
    executable = shutil.which("node")
    assert executable is not None, "Node must be installed."
    result = subprocess.run(  # noqa: S603 -- Execute the repository's fixed replay example.
        [executable, str(ROOT / "javascript/dist/examples/replay.js")],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert json.loads(result.stdout) == expected
    sys.stdout.write("Shared-engine replay passed: 20 valid fixtures and 20 failure envelopes.\n")


if __name__ == "__main__":
    main()
