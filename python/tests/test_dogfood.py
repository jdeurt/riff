import json
import subprocess
import sys
from pathlib import Path

from riff import FixtureFactory
from riff.factory import GenerationOptions

ROOT = Path(__file__).resolve().parents[2]
OPTIONS = FixtureFactory.from_file(
    ROOT / "schemas/run-options.schema.json",
    fixture_file=ROOT / "examples/riff-options.fixtures.yaml",
).as_model(GenerationOptions)


def test_riff_generated_options_drive_real_riff_batches() -> None:
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": 1, "maximum": 50})
    for options in OPTIONS.generate(count=30, seed=42):
        values = factory.generate(
            count=options.count,
            seed=options.seed,
            profile=options.profile,
            max_attempts=options.max_attempts,
        )
        assert len(values) == options.count
        for value in values:
            assert isinstance(value, int)
            assert 1 <= value <= 50
        assert values == factory.generate(
            count=options.count,
            seed=options.seed,
            profile=options.profile,
            max_attempts=options.max_attempts,
        )


def test_riff_generated_schemas_drive_constrained_generation() -> None:
    grammar = FixtureFactory.from_file(ROOT / "schemas/bounded-integer-schema.schema.json")
    for schema in grammar.generate(count=20, seed=42):
        assert isinstance(schema, dict)
        lower = schema["minimum"]
        upper = schema["maximum"]
        assert isinstance(lower, int)
        assert isinstance(upper, int)
        for value in FixtureFactory.from_schema(schema).generate(count=10, seed=7):
            assert isinstance(value, int)
            assert lower <= value <= upper


def test_riff_generated_invalid_options_are_rejected_by_its_cli(tmp_path: Path) -> None:
    contract = tmp_path / "contract.json"
    contract.write_text(
        json.dumps({"type": "integer", "minimum": 1, "maximum": 50}), encoding="utf-8"
    )
    flags = {
        "count": "--count",
        "seed": "--seed",
        "profile": "--profile",
        "max_attempts": "--max-attempts",
    }
    for case in OPTIONS.failure_cases(count=12, seed=42):
        assert not case.schema_valid
        assert isinstance(case.value, dict)
        command = [sys.executable, "-m", "riff", str(contract)]
        for name, value in case.value.items():
            if value is not None:
                command.extend([flags[name], str(value)])
        result = subprocess.run(command, capture_output=True, text=True, check=False)  # noqa: S603
        assert result.returncode == 2
        assert result.stdout == ""
        assert result.stderr
        assert "Traceback" not in result.stderr
