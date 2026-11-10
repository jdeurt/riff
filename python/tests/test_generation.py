from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import pytest
from jsonschema import Draft7Validator, FormatChecker

from riff import (
    FixtureFactory,
    GenerationError,
    JSONValue,
    Profile,
    ProviderContext,
    ProviderError,
    SchemaError,
)

CONTRACT = Path(__file__).resolve().parents[2] / "examples/vendor.yaml"


@pytest.mark.parametrize("profile", ["mixed", "realistic", "boundary"])
def test_openapi_response_batches_are_valid_and_replayable(profile: Profile) -> None:
    factory = FixtureFactory.from_file(CONTRACT, path="/customers/{id}")
    first = factory.generate(count=40, seed=42, profile=profile)
    assert first == factory.generate(count=40, seed=42, profile=profile)
    assert len(first) == 40
    for fixture in first:
        assert isinstance(fixture, dict)
        assert set(fixture) >= {"id", "name", "status", "balance_cents"}
        assert isinstance(fixture["id"], str)
        assert isinstance(fixture["name"], str)
        assert fixture["status"] in ("active", "inactive")
        assert isinstance(fixture["balance_cents"], int)
        assert 0 <= fixture["balance_cents"] <= 1000000
    assert first != factory.generate(count=40, seed=43, profile=profile)


@pytest.mark.parametrize(
    "schema",
    [
        {"type": "integer", "minimum": -4, "maximum": 11, "multipleOf": 3},
        {"type": "integer", "minimum": 0, "maximum": 20, "multipleOf": 1.5},
        {"type": "number", "minimum": 0.0001, "exclusiveMaximum": 0.0002},
        {"type": "number", "minimum": 0.3, "maximum": 0.3},
        {"type": "number", "minimum": -2, "maximum": 2, "multipleOf": 0.25},
        {"type": ["string", "null"], "minLength": 2, "maxLength": 6},
        {"type": "string", "pattern": "^[A-Z]{2}-[0-9]{3}$"},
        {
            "type": "array",
            "minItems": 2,
            "maxItems": 3,
            "uniqueItems": True,
            "items": {"enum": [False, 0, 1, True]},
        },
        {"anyOf": [{"const": "unknown"}, {"type": "integer", "minimum": 1}]},
        {"oneOf": [{"type": "string"}, {"type": "integer"}]},
        {
            "allOf": [
                {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}},
                {
                    "type": "object",
                    "required": ["name"],
                    "properties": {"name": {"type": "string"}},
                },
            ]
        },
    ],
)
def test_supported_constraints_hold_across_a_batch(schema: dict[str, JSONValue]) -> None:
    fixtures = FixtureFactory.from_schema(schema).generate(count=30, seed=2026)
    validator = Draft7Validator(schema, format_checker=FormatChecker())
    assert len(fixtures) == 30
    assert all(validator.is_valid(fixture) for fixture in fixtures)


def test_boundary_profile_explores_omission_null_and_numeric_endpoints() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "additionalProperties": False,
            "required": ["number"],
            "properties": {
                "number": {"type": "integer", "minimum": -10, "maximum": 10},
                "contact": {"type": ["string", "null"]},
            },
        }
    )
    fixtures: list[dict[str, JSONValue]] = []
    for fixture in factory.generate(count=100, seed=1, profile="boundary"):
        assert isinstance(fixture, dict)
        fixtures.append(fixture)
    assert {fixture["number"] for fixture in fixtures} == {-10, 0, 10}
    assert any("contact" not in fixture for fixture in fixtures)
    assert any("contact" in fixture and fixture["contact"] is None for fixture in fixtures)


def test_custom_provider_generates_whole_objects() -> None:
    def account(context: ProviderContext) -> JSONValue:
        return {"id": f"acct_{context.seed:016x}", "region": context.options["region"]}

    schema: dict[str, JSONValue] = {
        "type": "object",
        "required": ["id", "region"],
        "additionalProperties": False,
        "properties": {
            "id": {"type": "string", "pattern": "^acct_[0-9a-f]{16}$"},
            "region": {"const": "eu"},
        },
        "x-fixture": {"provider": "acme.account", "options": {"region": "eu"}},
    }
    factory = FixtureFactory.from_schema(schema, providers={"acme.account": account})
    fixtures = factory.generate(count=5, seed=7, profile="realistic")
    assert all(Draft7Validator(schema).is_valid(fixture) for fixture in fixtures)
    assert fixtures == factory.generate(count=5, seed=7, profile="realistic")
    identifiers: set[str] = set()
    for fixture in fixtures:
        assert isinstance(fixture, dict)
        identifier = fixture["id"]
        assert isinstance(identifier, str)
        identifiers.add(identifier)
    assert len(identifiers) == 5


def test_custom_providers_can_override_defaults() -> None:
    def email(_context: ProviderContext) -> JSONValue:
        return "fixture@example.test"

    factory = FixtureFactory.from_schema(
        {"type": "string", "format": "email", "x-fixture": {"provider": "email"}},
        providers={"email": email},
    )
    assert factory.generate(count=3, seed=7, profile="realistic") == ["fixture@example.test"] * 3


def test_custom_provider_exceptions_have_a_public_error() -> None:
    def broken(_context: ProviderContext) -> JSONValue:
        message = "upstream unavailable"
        raise RuntimeError(message)

    factory = FixtureFactory.from_schema(
        {"type": "integer", "x-fixture": {"provider": "broken"}}, providers={"broken": broken}
    )
    with pytest.raises(ProviderError, match=r"broken.*upstream unavailable"):
        factory.generate(seed=1, profile="realistic")


def test_provider_values_that_violate_constraints_exhaust_the_budget() -> None:
    def negative(_context: ProviderContext) -> JSONValue:
        return -1

    factory = FixtureFactory.from_schema(
        {"type": "integer", "minimum": 0, "x-fixture": {"provider": "negative"}},
        providers={"negative": negative},
    )
    with pytest.raises(GenerationError, match="3 attempts"):
        factory.generate(seed=1, max_attempts=3, profile="realistic")


def test_parallel_batches_do_not_change_replay() -> None:
    factory = FixtureFactory.from_file(CONTRACT, path="/customers/{id}")
    expected = factory.generate(count=5, seed=9)
    with ThreadPoolExecutor(max_workers=3) as executor:
        batches = list(executor.map(lambda _: factory.generate(count=5, seed=9), range(6)))
    assert all(batch == expected for batch in batches)


@pytest.mark.parametrize(
    "schema",
    [
        {"$ref": "https://example.test/schema.json"},
        {"$ref": "#/missing"},
        {"type": "string", "x-fixture": {"provider": "typo"}},
        {"type": "string", "x-fixture": {"examples": [42]}},
    ],
)
def test_bad_contracts_fail_before_generation(schema: dict[str, JSONValue]) -> None:
    with pytest.raises(SchemaError):
        FixtureFactory.from_schema(schema)


def test_unsatisfiable_contract_reports_failure_instead_of_invalid_data() -> None:
    factory = FixtureFactory.from_schema({"type": "integer", "minimum": 5, "maximum": 2})
    with pytest.raises(GenerationError, match="No valid fixture"):
        factory.generate(seed=0, max_attempts=2)
