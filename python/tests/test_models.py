import pytest
from pydantic import BaseModel, ConfigDict, Field
from typing_extensions import assert_type

from riff import (
    FailureCase,
    FixtureFactory,
    GenerationError,
    JSONValue,
    ProviderContext,
    ProviderError,
    SuccessCase,
)


class Account(BaseModel):
    model_config = ConfigDict(strict=True)
    seats: int = Field(
        ge=1,
        le=50,
        json_schema_extra={
            "x-fixture": {"fail": {"strategy": {"kind": "schema", "constraints": "violate"}}}
        },
    )


def test_model_first_generation_returns_inferred_models_and_raw_negative_cases() -> None:
    factory = FixtureFactory.for_model(Account)
    values = factory.generate(count=30, seed=1)
    assert_type(values, list[Account])
    assert all(1 <= value.seats <= 50 for value in values)
    cases = factory.success_cases(count=10, seed=7)
    assert_type(cases, list[SuccessCase[Account]])
    assert all(
        case.schema_valid and case.expectation == "pass" and 1 <= case.value.seats <= 50
        for case in cases
    )
    failures = factory.failure_cases(count=30, seed=1)
    assert_type(failures, list[FailureCase])
    observed: set[int] = set()
    for case in failures:
        assert not case.schema_valid
        assert isinstance(case.value, dict)
        seats = case.value["seats"]
        assert isinstance(seats, int)
        observed.add(seats)
    assert observed == {0, 51}


def test_model_binding_checks_the_model_in_addition_to_the_original_schema() -> None:
    factory = FixtureFactory.from_schema({"const": {"seats": 0}}).as_model(Account)
    with pytest.raises(GenerationError, match="bound model Account"):
        factory.generate(seed=7)


def test_model_binding_cannot_discard_required_schema_fields() -> None:
    class Projected(BaseModel):
        id: str

    factory = FixtureFactory.from_schema(
        {"const": {"id": "known", "required_data": "preserve"}}
    ).as_model(Projected)
    with pytest.raises(GenerationError, match=r"Serialized bound model.*original schema"):
        factory.generate(seed=7)


def test_custom_provider_contexts_cannot_change_the_next_invocation() -> None:
    def mutate_context(context: ProviderContext) -> JSONValue:
        nested = context.options["nested"]
        assert isinstance(nested, dict)
        original = nested["value"]
        nested["value"] = "mutated"
        return original

    factory = FixtureFactory.from_schema(
        {
            "type": "string",
            "x-fixture": {"provider": "isolated", "options": {"nested": {"value": "original"}}},
        },
        providers={"isolated": mutate_context},
    )
    assert factory.generate(count=5, seed=7, profile="realistic") == ["original"] * 5


def test_nonfinite_custom_data_is_a_provider_error() -> None:
    def nonfinite(_context: ProviderContext) -> JSONValue:
        return float("nan")

    factory = FixtureFactory.from_schema(
        {"type": "number", "x-fixture": {"provider": "nonfinite"}},
        providers={"nonfinite": nonfinite},
    )
    with pytest.raises(ProviderError, match="nonfinite"):
        factory.generate(seed=7, profile="realistic")
