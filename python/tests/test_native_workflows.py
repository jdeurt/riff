import subprocess
import sys
from pathlib import Path

import pytest

from riff import FixtureFactory, GenerationError, JSONValue, SchemaError


def test_native_numeric_generation_preserves_integer_constraints_above_double_precision() -> None:
    value = 9007199254740993
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": value, "maximum": value})
    assert factory.generate(count=3, seed=42, max_attempts=2) == [value, value, value]


def test_native_oracle_schema_errors_preserve_the_original_exception() -> None:
    sentinel = SchemaError("caller-owned sentinel")

    def oracle(_value: object) -> bool:
        raise sentinel

    with pytest.raises(SchemaError) as raised:
        FixtureFactory.from_schema(schema=True).minimize("failure", is_failure=oracle)
    assert raised.value is sentinel


def test_native_decimals_that_cannot_round_trip_fail_instead_of_returning_invalid_passes() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "number",
            "minimum": 9,
            "maximum": 9.1,
            "multipleOf": 0.1234567890123456,
        }
    )
    with pytest.raises(GenerationError, match="cannot preserve numeric output"):
        factory.generate(count=2, seed=42, max_attempts=2)


def test_both_alternative_groups_contribute_reachable_failure_declarations() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "string",
            "oneOf": [{"type": "string"}],
            "anyOf": [{"type": "string", "x-fixture": {"fail": {"examples": [42]}}}],
        }
    )
    cases = factory.failure_cases(count=3, seed=42, max_attempts=2)
    assert [case.value for case in cases] == [42, 42, 42]
    assert all(case.target == "" and not case.schema_valid for case in cases)


@pytest.mark.parametrize("metadata", [{"typo": True}, {"provider": "missing.provider"}])
def test_every_alternative_is_checked_before_sampling(metadata: dict[str, JSONValue]) -> None:
    with pytest.raises(SchemaError):
        FixtureFactory.from_schema(
            {
                "oneOf": [{"type": "string"}],
                "anyOf": [{"type": "string", "x-fixture": metadata}],
            }
        )


def test_failure_replacements_cannot_exceed_the_complete_payload_depth_limit() -> None:
    replacement: JSONValue = "leaf"
    for _ in range(32):
        replacement = [replacement]
    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "required": ["value"],
            "properties": {
                "value": {"type": "string", "x-fixture": {"fail": {"examples": [replacement]}}}
            },
        }
    )
    with pytest.raises(GenerationError, match="No reachable failure"):
        factory.failure_cases(seed=42, max_attempts=2)


def test_adding_an_optional_failure_field_cannot_exceed_the_payload_size_limit() -> None:
    factory = FixtureFactory.from_schema(
        {
            "const": {f"known_{index}": index for index in range(1024)},
            "properties": {"omitted": {"x-fixture": {"fail": {"examples": [0]}}}},
        }
    )
    with pytest.raises(GenerationError, match="No reachable failure"):
        factory.failure_cases(seed=42, max_attempts=2)


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux address-space limit protects this regression."
)
def test_native_minimization_work_is_bounded_before_the_next_oracle_call() -> None:
    script = """
import resource
from riff import FixtureFactory
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
original = ["x" * 1024 for _ in range(1024)]
result = FixtureFactory.from_schema(schema=True).minimize(
    original,
    is_failure=lambda value: value == original,
    max_evaluations=2,
)
assert result.value == original
assert result.evaluations == 2
assert result.budget_exhausted
print("bounded reducer passed")
"""
    result = subprocess.run(  # noqa: S603 -- Isolated regression with a fixed script and memory cap.
        [sys.executable, "-c", script], capture_output=True, text=True, check=False, timeout=15
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout == "bounded reducer passed\n"


def test_successful_if_condition_contributes_reachable_failure_metadata() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "required": ["x"],
            "properties": {"x": {"type": "integer"}},
            "if": {
                "properties": {
                    "x": {"type": "integer", "x-fixture": {"fail": {"examples": ["bad"]}}}
                }
            },
            "then": True,
        }
    )
    cases = factory.failure_cases(count=3, seed=42, max_attempts=2)
    assert all(case.target == "/x" and not case.schema_valid for case in cases)


def test_minimization_work_exhaustion_retains_the_confirmed_reproducer() -> None:
    original: JSONValue = ["kept"] * 300
    result = FixtureFactory.from_schema({"const": original}).minimize(
        original, is_failure=lambda _value: True, max_evaluations=2
    )
    assert result.value == original
    assert result.evaluations == 1
    assert result.budget_exhausted


@pytest.mark.parametrize("content", ["", "null", "[]", "42"])
def test_file_schema_shapes_raise_the_public_contract_error(tmp_path: Path, content: str) -> None:
    path = tmp_path / "contract.yaml"
    path.write_text(content)
    with pytest.raises(SchemaError, match="object or boolean"):
        FixtureFactory.from_file(path)


@pytest.mark.skipif(
    sys.platform != "linux", reason="Linux resource limits isolate parser regressions."
)
@pytest.mark.parametrize(
    "content",
    ["- " * 20000 + "0\n", "type: integer\nminimum: 0e-1000000000\nmaximum: 1\n"],
    ids=["compact-block-depth", "zero-with-extreme-exponent"],
)
def test_resource_exhausting_files_raise_a_contract_error(tmp_path: Path, content: str) -> None:
    path = tmp_path / "contract.yaml"
    path.write_text(content)
    script = """
import resource
import sys
from riff import FixtureFactory, SchemaError
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_CPU, (2, 2))
try:
    factory = FixtureFactory.from_file(sys.argv[1])
    factory.generate(seed=42, max_attempts=1)
except SchemaError:
    print("rejected")
else:
    raise AssertionError("Invalid resource demands were accepted")
"""
    result = subprocess.run(  # noqa: S603 -- Fixed regression script with OS resource limits.
        [sys.executable, "-c", script, str(path)],
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout == "rejected\n"


def test_minimization_tries_zero_for_supported_arbitrary_precision_integers() -> None:
    calls: list[JSONValue] = []

    def failure(value: JSONValue) -> bool:
        calls.append(value)
        return isinstance(value, int) and not isinstance(value, bool)

    result = FixtureFactory.from_schema(schema=True).minimize(
        10**1000, is_failure=failure, max_evaluations=5
    )
    assert isinstance(result.value, int)
    assert not isinstance(result.value, bool)
    assert result.value == 0
    assert result.evaluations == len(calls)
    assert 1 < result.evaluations <= 5
    assert any(
        isinstance(value, int) and not isinstance(value, bool) and value == 0 for value in calls
    )


@pytest.mark.skipif(sys.platform != "linux", reason="Linux memory limits isolate this regression.")
def test_large_annotations_do_not_amplify_reference_scope_memory() -> None:
    script = """
import resource
from riff import FixtureFactory
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
schema = {
    "$id": "https://example.test/root",
    "$dynamicAnchor": "node",
    "description": "x" * (1024 * 1024),
    "type": "integer",
    "const": 7,
    "$defs": {f"N{index}": {"const": 7} for index in range(1000)},
    "allOf": [{"$ref": f"#/$defs/N{index}"} for index in range(1000)],
}
assert FixtureFactory.from_schema(schema).generate(seed=42) == [7]
print("bounded reference memory passed")
"""
    result = subprocess.run(  # noqa: S603 -- Fixed regression script with an OS memory cap.
        [sys.executable, "-c", script], capture_output=True, text=True, check=False, timeout=20
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout == "bounded reference memory passed\n"
