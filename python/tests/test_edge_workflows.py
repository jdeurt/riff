import json
import sys
from pathlib import Path

import pytest
from jsonschema import Draft7Validator
from pydantic import TypeAdapter

from riff import FixtureFactory, GenerationError, JSONValue, SchemaError


def test_extreme_number_failure_is_still_serializable_json() -> None:
    schema: dict[str, JSONValue] = {
        "type": "number",
        "maximum": sys.float_info.max,
        "x-fixture": {"fail": {"strategy": {"kind": "schema", "constraints": "violate"}}},
    }
    case = FixtureFactory.from_schema(schema).failure_cases(seed=1)[0]
    encoded = json.dumps(case.value, allow_nan=False)
    assert TypeAdapter[JSONValue](JSONValue).validate_json(encoded) == case.value
    assert not case.schema_valid
    assert not Draft7Validator(schema).is_valid(case.value)


def test_sidecar_pointer_treats_percent_characters_literally(tmp_path: Path) -> None:
    sidecar = tmp_path / "fixtures.yaml"
    sidecar.write_text(
        "version: '0.1'\n"
        "fields:\n  /properties/a%20b%2Fc:\n"
        "    strategy: {kind: values, values: [attached]}\n",
        encoding="utf-8",
    )
    factory = FixtureFactory.from_schema(
        {
            "type": "object",
            "required": ["a%20b%2Fc"],
            "additionalProperties": False,
            "properties": {"a%20b%2Fc": {"type": "string"}},
        },
        fixture_file=sidecar,
    )
    assert factory.generate(seed=1) == [{"a%20b%2Fc": "attached"}]


def test_additional_properties_avoid_reserved_names_and_keep_their_value_contract() -> None:
    schema: dict[str, JSONValue] = {
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "properties": {f"extra_{index}": False for index in range(101)},
        "additionalProperties": {"const": 17},
    }
    fixtures = FixtureFactory.from_schema(schema).generate(count=3, seed=1, max_attempts=3)
    for fixture in fixtures:
        assert isinstance(fixture, dict)
        assert len(fixture) == 1
        assert list(fixture.values()) == [17]
        assert not set(fixture).intersection(f"extra_{index}" for index in range(101))


def test_nested_collection_generation_cannot_exceed_the_documented_candidate_budget() -> None:
    factory = FixtureFactory.from_schema(
        {"type": "array", "minItems": 101, "items": {"type": "integer"}}
    )
    with pytest.raises(GenerationError, match=r"candidate.*budget"):
        factory.generate(seed=1, max_attempts=1)


def test_constraint_violation_strategy_cannot_mislabel_successful_values() -> None:
    with pytest.raises(SchemaError, match="explicit failure source"):
        FixtureFactory.from_schema(
            {
                "type": "integer",
                "minimum": 1,
                "x-fixture": {"strategy": {"kind": "schema", "constraints": "violate"}},
            }
        )
