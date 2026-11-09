import asyncio

import httpx
import pytest
from pydantic import BaseModel, Field, TypeAdapter
from typing_extensions import assert_type

from examples.app import create_app
from examples.published_openapi import configure_public_openapi
from riff import (
    Failure,
    FilterStrategy,
    FixtureFactory,
    JSONValue,
    SchemaStrategy,
    ValuesStrategy,
    fixture,
    strip_fixture_metadata,
)


class TypedRegistration(BaseModel):
    name: str = Field(
        min_length=1,
        max_length=64,
        json_schema_extra={
            **fixture(
                provider="full-name",
                fail=Failure(examples=["", None]),
            )
        },
    )
    seats: int = Field(
        ge=1,
        le=50,
        json_schema_extra=fixture(
            fail=Failure(
                strategy=SchemaStrategy(kind="schema", constraints="violate"),
            )
        ),
    )


def test_typed_metadata_builders_drive_real_model_generation_and_failure_cases() -> None:
    factory = FixtureFactory.for_model(TypedRegistration)
    values = factory.generate(count=20, seed=42)
    assert_type(values, list[TypedRegistration])
    assert all(value.name and 1 <= value.seats <= 50 for value in values)
    targets: set[str] = set()
    for case in factory.failure_cases(count=30, seed=42):
        assert not case.schema_valid
        targets.add(case.target)
        assert isinstance(case.value, dict)
        if case.target == "/seats":
            assert case.value["seats"] in (0, 51)
        else:
            assert case.target == "/name"
            assert case.value["name"] in ("", None)
    assert targets == {"/name", "/seats"}


def test_metadata_builder_validates_source_combinations_before_publication() -> None:
    with pytest.raises(ValueError, match="require a provider"):
        fixture(locale="en-US")


def test_publication_strips_schema_annotations_but_preserves_constraints_and_payload_data() -> None:
    document: dict[str, JSONValue] = {
        "openapi": "3.1.0",
        "components": {
            "schemas": {
                "Payload": {
                    "type": "object",
                    "additionalProperties": False,
                    "x-fixture": {"provider": "whole-object"},
                    "properties": {
                        "x-fixture": {
                            "type": "integer",
                            "minimum": 1,
                            "x-fixture": {"examples": [1]},
                        }
                    },
                    "examples": [{"x-fixture": "literal payload data"}],
                }
            }
        },
    }
    published = strip_fixture_metadata(document)
    assert published == {
        "openapi": "3.1.0",
        "components": {
            "schemas": {
                "Payload": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {"x-fixture": {"type": "integer", "minimum": 1}},
                    "examples": [{"x-fixture": "literal payload data"}],
                }
            }
        },
    }
    components = document["components"]
    assert isinstance(components, dict)
    schemas = components["schemas"]
    assert isinstance(schemas, dict)
    payload = schemas["Payload"]
    assert isinstance(payload, dict)
    assert "x-fixture" in payload


def test_fastapi_publishes_a_clean_schema_while_model_fixtures_keep_working() -> None:
    async def published_schema() -> httpx.Response:
        with httpx.Client() as vendor:
            app = create_app(vendor)
            configure_public_openapi(app)
            async with httpx.AsyncClient(
                transport=httpx.ASGITransport(app=app), base_url="http://testserver"
            ) as client:
                return await client.get("/openapi.json")

    response = asyncio.run(published_schema())
    assert response.status_code == 200
    document = TypeAdapter[dict[str, JSONValue]](dict[str, JSONValue]).validate_json(
        response.content
    )
    components = document["components"]
    assert isinstance(components, dict)
    schemas = components["schemas"]
    assert isinstance(schemas, dict)
    registration = schemas["Registration"]
    assert isinstance(registration, dict)
    properties = registration["properties"]
    assert isinstance(properties, dict)
    seats = properties["seats"]
    assert isinstance(seats, dict)
    assert "x-fixture" not in seats
    assert seats["minimum"] == 1
    assert seats["maximum"] == 50
    name = properties["name"]
    assert isinstance(name, dict)
    assert "x-fixture" not in name
    assert name["minLength"] == 1
    assert name["maxLength"] == 64
    cases = FixtureFactory.for_model(TypedRegistration).failure_cases(count=5, seed=42)
    assert all(not case.schema_valid for case in cases)


def test_programmatic_filters_preserve_their_predicate_schema_and_types() -> None:

    source = FilterStrategy.where(
        ValuesStrategy(kind="values", values=[-1, 10, 11]),
        schema={"type": "integer", "minimum": 10},
    )

    class Filtered(BaseModel):
        number: int = Field(json_schema_extra=fixture(strategy=source))

    values = FixtureFactory.for_model(Filtered).generate(count=30, seed=42)
    assert {value.number for value in values} == {10, 11}


def test_nullable_model_fields_accept_default_semantic_providers() -> None:
    class OptionalName(BaseModel):
        name: str | None = Field(json_schema_extra=fixture(provider="full-name", examples=[None]))

    values = FixtureFactory.for_model(OptionalName).generate(count=30, seed=42, profile="realistic")
    assert any(value.name is None for value in values)
    assert any(isinstance(value.name, str) and value.name for value in values)
