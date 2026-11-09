import subprocess
import sys
from pathlib import Path

from pydantic import TypeAdapter

from riff import JSONValue


def test_cli_generates_replayable_payloads_and_labeled_failure_jsonl() -> None:
    contract = Path(__file__).resolve().parents[2] / "examples/endpoint-vendor.yaml"
    command = [
        sys.executable,
        "-m",
        "riff",
        str(contract),
        "--path",
        "/customers/{id}",
        "--count",
        "4",
        "--seed",
        "42",
    ]
    first = subprocess.run(command, capture_output=True, text=True, check=True)  # noqa: S603
    replay = subprocess.run(command, capture_output=True, text=True, check=True)  # noqa: S603
    assert replay.stdout == first.stdout
    assert first.stderr == ""
    bodies = TypeAdapter[list[JSONValue]](list[JSONValue]).validate_json(first.stdout)
    assert len(bodies) == 4
    for body in bodies:
        assert isinstance(body, dict)
        assert body["status"] == "active"
    failure = subprocess.run(  # noqa: S603
        [*command, "--expectation", "fail", "--jsonl"], capture_output=True, text=True, check=True
    )
    cases = [
        TypeAdapter[JSONValue](JSONValue).validate_json(line)
        for line in failure.stdout.splitlines()
    ]
    assert len(cases) == 4
    for case in cases:
        assert isinstance(case, dict)
        assert case["expectation"] == "fail"
        assert isinstance(case["target"], str)
        assert case["target"] != ""


def test_cli_failure_has_no_partial_json_on_stdout(tmp_path: Path) -> None:
    contract = tmp_path / "impossible.json"
    contract.write_text('{"type":"integer","minimum":5,"maximum":2}', encoding="utf-8")
    result = subprocess.run(  # noqa: S603
        [sys.executable, "-m", "riff", str(contract), "--max-attempts", "2"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 2
    assert result.stdout == ""
    assert "No valid fixture" in result.stderr
    assert "Traceback" not in result.stderr
