import pytest

from riff import FixtureFactory, GenerationError, JSONValue


def test_choice_and_filter_compose_without_leaking_rejected_values() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "integer",
            "x-fixture": {
                "strategy": {
                    "kind": "filter",
                    "schema": {"minimum": 10},
                    "strategy": {
                        "kind": "one-of",
                        "strategies": [
                            {"kind": "values", "values": [-1, 10]},
                            {"kind": "values", "values": [11, 12]},
                        ],
                    },
                }
            },
        }
    )
    values = factory.generate(count=80, seed=5)
    assert set(values) == {10, 11, 12}


def test_provider_strategy_can_be_composed_with_explicit_values() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "string",
            "format": "email",
            "x-fixture": {
                "strategy": {
                    "kind": "one-of",
                    "strategies": [
                        {"kind": "provider", "name": "email"},
                        {"kind": "values", "values": ["explicit@example.test"]},
                    ],
                }
            },
        }
    )
    values = factory.generate(count=20, seed=9, profile="boundary")
    assert "explicit@example.test" in values
    assert any(value != "explicit@example.test" for value in values)


def test_schema_strategy_respects_field_constraints() -> None:
    factory = FixtureFactory.from_schema(
        {
            "type": "integer",
            "minimum": 10,
            "maximum": 12,
            "x-fixture": {"strategy": {"kind": "schema"}},
        }
    )
    assert set(factory.generate(count=50, seed=4)) == {10, 11, 12}


def test_impossible_filter_is_a_generation_failure() -> None:
    schema: dict[str, JSONValue] = {
        "type": "integer",
        "x-fixture": {
            "strategy": {
                "kind": "filter",
                "schema": {"minimum": 10},
                "strategy": {"kind": "values", "values": [0]},
            }
        },
    }
    with pytest.raises(GenerationError, match=r"filter.*budget"):
        FixtureFactory.from_schema(schema).generate(seed=0, max_attempts=2)
