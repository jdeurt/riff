import gc
import subprocess
import sys
import weakref
from collections.abc import Iterator
from pathlib import Path
from types import MappingProxyType
from typing import Literal, TypeVar

import pytest
from typing_extensions import assert_type, override

from riff import (
    ChoiceStrategy,
    Failure,
    FailureCase,
    FilterStrategy,
    FixtureFactory,
    GenerationError,
    JSONValue,
    MinimizationResult,
    ProviderContext,
    ProviderError,
    SchemaError,
    SchemaStrategy,
    SuccessCase,
    ValuesStrategy,
    Violation,
    fixture,
)

ItemT = TypeVar("ItemT")


class IntrinsicList(list[ItemT]):
    @override
    def __iter__(self) -> Iterator[ItemT]:
        message = "Subclass iteration must not run"
        raise AssertionError(message)


def test_native_numeric_generation_preserves_integer_constraints_above_double_precision() -> None:
    value = 9007199254740993
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": value, "maximum": value})
    assert factory.generate(count=3, seed=42, max_attempts=2) == [value, value, value]


def test_json_subclasses_preserve_values_without_running_conversion_hooks() -> None:
    class Number(int):
        @override
        def bit_length(self) -> int:
            message = "Subclass bit_length must not run"
            raise AssertionError(message)

        @override
        def to_bytes(self, *_args: object, **_kwargs: object) -> bytes:
            message = "Subclass to_bytes must not run"
            raise AssertionError(message)

        @override
        def __int__(self) -> int:
            message = "Subclass __int__ must not run"
            raise AssertionError(message)

    value = Number(7)
    assert FixtureFactory.from_schema({"const": value}).generate(seed=42) == [7]
    assert FixtureFactory.from_schema({"const": IntrinsicList([value])}).generate(seed=42) == [[7]]
    source = ValuesStrategy(kind="values", values=IntrinsicList([7]))
    assert source.values == [7]
    choices = ChoiceStrategy(kind="one-of", strategies=IntrinsicList([source]))
    assert FixtureFactory.from_schema(fixture(strategy=choices)).generate(seed=42) == [7]
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": 1, "maximum": 50})
    assert factory.generate(count=10, seed=value) == factory.generate(count=10, seed=7)


@pytest.mark.skipif(sys.platform != "linux", reason="Linux resource limits bound this regression.")
def test_oversized_host_integers_fail_before_decimal_conversion() -> None:
    script = """
import resource
from riff import FixtureFactory
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
resource.setrlimit(resource.RLIMIT_CPU, (2, 2))
value = 1 << 64000000
for action in (
    lambda: FixtureFactory.from_schema({"const": value}),
    lambda: FixtureFactory.from_schema(True).generate(seed=value),
):
    try:
        action()
    except ValueError as error:
        assert "numeric allocation limits" in str(error)
    else:
        raise AssertionError("Oversized integer was accepted")
"""
    completed = subprocess.run(  # noqa: S603 -- Fixed script with OS memory and CPU caps.
        [sys.executable, "-c", script], capture_output=True, text=True, timeout=10, check=False
    )
    assert completed.returncode == 0, completed.stdout + completed.stderr


@pytest.mark.skipif(sys.platform != "linux", reason="Unix paths preserve undecodable bytes.")
def test_undecodable_file_paths_raise_argument_errors() -> None:
    path = Path("\udcff")
    with pytest.raises(ValueError, match="valid UTF-8"):
        FixtureFactory.from_file(path)
    with pytest.raises(ValueError, match="valid UTF-8"):
        FixtureFactory.from_schema(schema=True, fixture_file=path)


def test_provider_objects_and_mapping_registrations_preserve_exception_causes() -> None:
    sentinel = RuntimeError("provider failed")

    class BrokenProvider:
        def __call__(self, _context: ProviderContext) -> JSONValue:
            raise sentinel

    providers = MappingProxyType({"broken": BrokenProvider()})
    factory = FixtureFactory.from_schema(
        {"type": "string", "x-fixture": {"provider": "broken"}}, providers=providers
    )
    with pytest.raises(ProviderError, match=r"broken.*provider failed") as raised:
        factory.generate(seed=42, profile="realistic")
    assert raised.value.__cause__ is sentinel


@pytest.mark.parametrize("sentinel", [KeyboardInterrupt("cancel"), SystemExit(7), GeneratorExit()])
def test_provider_cancellation_and_exit_preserve_the_original_exception(
    sentinel: BaseException,
) -> None:
    def interrupted(_context: ProviderContext) -> JSONValue:
        raise sentinel

    factory = FixtureFactory.from_schema(
        {"type": "string", "x-fixture": {"provider": "interrupted"}},
        providers={"interrupted": interrupted},
    )
    with pytest.raises(type(sentinel)) as raised:
        factory.generate(seed=42, profile="realistic")
    assert raised.value is sentinel


def test_case_payloads_are_independent_json_trees_with_typed_labels() -> None:
    original: JSONValue = {"value": [None, True, 10**100, 0.25, "é"]}
    factory = FixtureFactory.from_schema({"const": original})
    case = factory.success_cases(seed=42)[0]
    assert_type(case.expectation, Literal["pass"])
    assert_type(case.schema_valid, Literal[True])
    assert case.value == original
    assert case.payload == original
    assert case.value is not case.payload
    assert isinstance(case.value, dict)
    case.value["value"] = "changed"
    assert case.payload == original
    assert factory.generate(seed=42) == [original]


def test_public_context_and_result_constructors_keep_positional_arguments() -> None:
    violation = Violation("/value", "invalid")
    assert violation.path == "/value"
    assert violation.message == "invalid"
    success = SuccessCase(1, 1)
    assert success.value == success.payload == 1
    assert success.expectation == "pass"
    schema_valid = False
    failure = FailureCase(0, schema_valid, (violation,), "/value", "too small")
    assert failure.value == failure.payload == 0
    assert failure.violations == (violation,)
    assert failure.reason == "too small"
    budget_exhausted = False
    result = MinimizationResult(1, 2, budget_exhausted)
    assert result.value == 1
    assert result.evaluations == 2
    assert not result.budget_exhausted
    context = ProviderContext(1, "en-US", MappingProxyType({}), MappingProxyType({}))
    assert context.seed == 1
    assert context.locale == "en-US"
    assert context.schema == context.options == {}


def test_provider_and_result_reference_cycles_can_be_collected() -> None:
    class LinkedProvider:
        factory: FixtureFactory | None = None

        def __call__(self, _context: ProviderContext) -> JSONValue:
            return "valid"

    provider = LinkedProvider()
    factory = FixtureFactory.from_schema(
        {"type": "string", "x-fixture": {"provider": "linked"}}, providers={"linked": provider}
    )
    assert factory.generate(seed=42, profile="realistic") == ["valid"]
    provider.factory = factory
    provider_reference = weakref.ref(provider)
    del provider, factory
    gc.collect()
    assert provider_reference() is None

    class LinkedValue:
        case: SuccessCase[object] | None = None

    value = LinkedValue()
    case = SuccessCase(value, None)
    value.case = case
    value_reference = weakref.ref(value)
    del value, case
    gc.collect()
    assert value_reference() is None


def test_nested_strategy_builders_preserve_portable_metadata_and_failure_reasons() -> None:
    alternatives = ChoiceStrategy(
        kind="one-of",
        strategies=[ValuesStrategy(kind="values", values=[1, 2]), SchemaStrategy(kind="schema")],
    )
    strategy = FilterStrategy.where(alternatives, schema={"minimum": 2})
    metadata = fixture(strategy=strategy, fail=Failure(examples=(0,), reason="at least two"))
    assert metadata == {
        "x-fixture": {
            "strategy": {
                "kind": "filter",
                "strategy": {
                    "kind": "one-of",
                    "strategies": [
                        {"kind": "values", "values": [1, 2]},
                        {"kind": "schema", "constraints": "respect"},
                    ],
                },
                "schema": {"minimum": 2},
            },
            "fail": {"examples": [0], "reason": "at least two"},
        }
    }
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": 2, **metadata})
    for value in factory.generate(count=5, seed=42):
        assert isinstance(value, int)
        assert value >= 2
    case = factory.failure_cases(seed=42)[0]
    assert_type(case.expectation, Literal["fail"])
    assert case.value == 0
    assert not case.schema_valid
    assert case.reason == "at least two"


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
