from pathlib import Path

import pytest
from jsonschema import Draft7Validator

from riff import FixtureFactory, GenerationError, JSONValue, ProviderContext


def test_declared_failures_separate_schema_validity_from_consumer_expectation() -> None:
    factory = FixtureFactory.from_file(
        Path(__file__).resolve().parents[2] / "examples/endpoint-vendor.yaml",
        path="/customers/{id}",
    )
    cases = factory.generate_cases(count=50, seed=42, expectation="fail")
    assert {case.target for case in cases} == {"/name", "/status"}
    assert all(case.expectation == "fail" and case.reason for case in cases)
    for case in cases:
        assert isinstance(case.value, dict)
        assert isinstance(case.value["id"], str)
        if case.target == "/name":
            assert not case.schema_valid
            assert case.violations[0].path == "/name"
            assert case.value["status"] == "active"
        else:
            assert case.schema_valid
            assert case.violations == ()
            assert case.value["status"] == "inactive"
            assert isinstance(case.value["name"], str)
    assert cases == factory.generate_cases(count=50, seed=42, expectation="fail")


def test_custom_failure_provider_can_return_an_invalid_whole_object() -> None:
    def malformed(_context: ProviderContext) -> JSONValue:
        return {"error": "missing record"}

    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "required": ["id"],
            "properties": {"id": {"type": "string"}},
            "x-fixture": {"fail": {"provider": "malformed"}},
        },
        providers={"malformed": malformed},
    )
    case = factory.generate_cases(seed=5, expectation="fail")[0]
    assert case.value == {"error": "missing record"}
    assert case.target == ""
    assert not case.schema_valid
    assert case.expectation == "fail"


def test_failures_in_arrays_report_escaped_json_pointers() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "required": ["a/b~c"],
            "properties": {
                "a/b~c": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 1,
                    "items": {"type": "string", "x-fixture": {"fail": {"examples": [None]}}},
                }
            },
        }
    )
    case = factory.generate_cases(seed=1, expectation="fail")[0]
    assert case.target == "/a~1b~0c/0"
    assert case.violations[0].path == case.target
    assert isinstance(case.value, dict)
    assert case.value["a/b~c"] == [None]


def test_missing_failure_declarations_do_not_create_mislabeled_cases() -> None:
    with pytest.raises(GenerationError, match="No reachable failure"):
        FixtureFactory.from_schema({"type": "string"}).generate_cases(
            seed=1, expectation="fail", max_attempts=2
        )


def test_minimization_retains_failure_and_schema_without_changing_input() -> None:
    schema: dict[str, JSONValue] = {
        "type": "object",
        "required": ["name"],
        "properties": {"name": {"type": "string", "minLength": 1}},
    }
    factory = FixtureFactory.from_schema(schema)
    original: JSONValue = {"name": "prefix!suffix", "unrelated": [1, 2, 3]}

    def fails(value: JSONValue) -> bool:
        if not isinstance(value, dict):
            return False
        name = value.get("name")
        return isinstance(name, str) and "!" in name

    result = factory.minimize(original, is_failure=fails)
    assert result.value == {"name": "!"}
    assert Draft7Validator(schema).is_valid(result.value)
    assert fails(result.value)
    assert original == {"name": "prefix!suffix", "unrelated": [1, 2, 3]}
    assert result.evaluations <= 100
    assert not result.budget_exhausted


def test_minimization_can_preserve_a_malformed_failure_and_respect_its_budget() -> None:
    factory = FixtureFactory.from_schema({"type": "string"})

    def fails(value: JSONValue) -> bool:
        return isinstance(value, list) and len(value) > 0

    result = factory.minimize(
        ["bad", "extra"], is_failure=fails, preserve_schema=False, max_evaluations=2
    )
    assert result.evaluations == 2
    assert result.budget_exhausted
    assert fails(result.value)
    with pytest.raises(ValueError, match="Initial value does not reproduce"):
        factory.minimize("fine", is_failure=lambda _: False)


@pytest.mark.parametrize(
    ("schema", "target"),
    [
        (
            {
                "type": "object",
                "required": ["x"],
                "properties": {"x": {"type": "integer"}},
                "if": True,
                "then": {
                    "properties": {
                        "x": {"type": "integer", "x-fixture": {"fail": {"examples": ["bad"]}}}
                    }
                },
            },
            "/x",
        ),
        (
            {
                "type": "object",
                "required": ["x_1"],
                "maxProperties": 1,
                "patternProperties": {
                    "^x_": {"type": "integer", "x-fixture": {"fail": {"examples": ["bad"]}}}
                },
                "additionalProperties": False,
            },
            "/x_1",
        ),
        (
            {
                "type": "object",
                "minProperties": 1,
                "maxProperties": 1,
                "additionalProperties": {
                    "type": "integer",
                    "x-fixture": {"fail": {"examples": ["bad"]}},
                },
            },
            None,
        ),
        (
            {
                "type": "object",
                "minProperties": 1,
                "maxProperties": 1,
                "unevaluatedProperties": {
                    "type": "integer",
                    "x-fixture": {"fail": {"examples": ["bad"]}},
                },
            },
            None,
        ),
        (
            {
                "type": "array",
                "minItems": 1,
                "maxItems": 1,
                "contains": {"type": "integer", "x-fixture": {"fail": {"examples": ["bad"]}}},
            },
            "/0",
        ),
        (
            {
                "type": "array",
                "prefixItems": [{"const": "head"}],
                "minItems": 2,
                "maxItems": 2,
                "unevaluatedItems": {
                    "type": "integer",
                    "x-fixture": {"fail": {"examples": ["bad"]}},
                },
            },
            "/1",
        ),
        (
            {
                "type": "object",
                "required": ["a"],
                "properties": {"a": {"const": 1}, "b": {"type": "integer"}},
                "additionalProperties": False,
                "dependentSchemas": {
                    "a": {
                        "required": ["b"],
                        "properties": {
                            "b": {"type": "integer", "x-fixture": {"fail": {"examples": ["bad"]}}}
                        },
                    }
                },
            },
            "/b",
        ),
    ],
)
def test_failure_sources_in_applicable_schema_scopes_reach_their_payload_fields(
    schema: dict[str, JSONValue], target: str | None
) -> None:
    cases = FixtureFactory.from_schema(schema).failure_cases(count=5, seed=42)
    assert len(cases) == 5
    for case in cases:
        assert not case.schema_valid
        assert case.violations
        if target is not None:
            assert case.target == target
        else:
            assert case.target.startswith("/")


def test_nested_constraint_violations_use_the_parent_schema_dialect() -> None:
    schema: dict[str, JSONValue] = {
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "required": ["x"],
        "additionalProperties": False,
        "properties": {
            "x": {
                "type": "string",
                "format": "uuid",
                "x-fixture": {"fail": {"strategy": {"kind": "schema", "constraints": "violate"}}},
            }
        },
    }
    for case in FixtureFactory.from_schema(schema).failure_cases(count=20, seed=0):
        assert not case.schema_valid
        assert isinstance(case.value, dict)
        assert not isinstance(case.value["x"], str)
