from typing import Literal

import pytest
from jsonschema import Draft202012Validator
from jsonschema.validators import validator_for
from pydantic import BaseModel

from riff import FixtureFactory, JSONValue


@pytest.mark.parametrize("additional", [False, True, None])
def test_additional_properties_follow_the_declared_policy(*, additional: bool | None) -> None:
    schema: dict[str, JSONValue] = {
        "type": "object",
        "required": ["known"],
        "properties": {"known": {"const": "expected"}},
    }
    if additional is not None:
        schema["additionalProperties"] = additional
    observed_extra = False
    for value in FixtureFactory.from_schema(schema).generate(count=40, seed=42):
        assert isinstance(value, dict)
        assert value["known"] == "expected"
        extras = set(value) - {"known"}
        observed_extra = observed_extra or bool(extras)
        if additional is False:
            assert extras == set()
    assert observed_extra is (additional is not False)


def test_additional_property_schemas_constrain_the_generated_garbage() -> None:
    schema: dict[str, JSONValue] = {
        "type": "object",
        "required": ["known"],
        "properties": {"known": {"const": "expected"}},
        "additionalProperties": {"type": "integer", "minimum": 10, "maximum": 20},
    }
    extras: list[int] = []
    for value in FixtureFactory.from_schema(schema).generate(count=30, seed=42):
        assert isinstance(value, dict)
        for name, extra in value.items():
            if name != "known":
                assert isinstance(extra, int)
                assert 10 <= extra <= 20
                extras.append(extra)
    assert extras


@pytest.mark.parametrize(
    "schema",
    [
        {
            "type": "object",
            "required": ["x_1"],
            "patternProperties": {"^x_": {"const": "ok"}},
            "additionalProperties": False,
        },
        {
            "type": "object",
            "minProperties": 1,
            "patternProperties": {"^x_[0-9]{2}$": {"type": "integer", "minimum": 0, "maximum": 9}},
            "additionalProperties": False,
        },
        {
            "type": "object",
            "required": ["x_1"],
            "patternProperties": {"^x_": {"type": "integer"}, "[0-9]$": {"minimum": 10}},
            "additionalProperties": False,
        },
        {"type": "object", "minProperties": 1, "propertyNames": {"pattern": "^key_[a-z]{3}$"}},
        {
            "type": "object",
            "required": ["a"],
            "properties": {"a": {"const": 1}, "b": {"const": 2}},
            "dependentRequired": {"a": ["b"]},
            "additionalProperties": False,
        },
        {
            "type": "object",
            "required": ["a"],
            "properties": {"a": {"const": 1}, "b": {"const": 2}},
            "dependentSchemas": {"a": {"required": ["b"]}},
            "additionalProperties": False,
        },
        {
            "type": "array",
            "prefixItems": [{"const": "head"}, {"type": "integer"}],
            "items": False,
            "minItems": 2,
        },
        {
            "type": "array",
            "items": {"type": "integer", "minimum": 0, "maximum": 10},
            "contains": {"minimum": 8},
            "minContains": 2,
            "maxContains": 2,
            "minItems": 3,
            "maxItems": 3,
        },
        {
            "type": "array",
            "items": {"type": "integer", "minimum": 0, "maximum": 10},
            "contains": {"minimum": 8},
            "minContains": 0,
            "maxContains": 0,
            "minItems": 3,
            "maxItems": 3,
        },
        {
            "type": "object",
            "allOf": [
                {"properties": {"a": {"type": "integer"}}, "required": ["a"]},
                {"properties": {"b": {"type": "string"}}, "required": ["b"]},
            ],
            "unevaluatedProperties": False,
        },
        {"type": "object", "minProperties": 1, "unevaluatedProperties": {"const": 17}},
        {"type": "object", "additionalProperties": True, "unevaluatedProperties": False},
        {
            "type": "array",
            "prefixItems": [{"const": "head"}, {"type": "boolean"}],
            "minItems": 2,
            "unevaluatedItems": False,
        },
        {
            "type": "array",
            "prefixItems": [{"const": "head"}],
            "minItems": 2,
            "unevaluatedItems": {"const": 17},
        },
        {"type": "string", "minLength": 1, "maxLength": 10, "not": {"enum": ["forbidden", "x"]}},
    ],
)
def test_current_json_schema_constraints_are_respected(schema: dict[str, JSONValue]) -> None:
    values = FixtureFactory.from_schema(schema).generate(count=20, seed=42)
    validator = Draft202012Validator(schema, format_checker=Draft202012Validator.FORMAT_CHECKER)
    assert len(values) == 20
    assert all(validator.is_valid(value) for value in values)


def test_conditionals_explore_both_branches_with_matching_values() -> None:
    schema: dict[str, JSONValue] = {
        "type": "object",
        "required": ["kind", "value"],
        "additionalProperties": False,
        "properties": {"kind": {"enum": ["small", "large"]}, "value": {"type": "integer"}},
        "if": {"properties": {"kind": {"const": "small"}}, "required": ["kind"]},
        "then": {"properties": {"value": {"maximum": 2}}},
        "else": {"properties": {"value": {"minimum": 10}}},
    }
    observed: set[str] = set()
    for value in FixtureFactory.from_schema(schema).generate(count=40, seed=42):
        assert isinstance(value, dict)
        kind = value["kind"]
        number = value["value"]
        assert isinstance(kind, str)
        assert isinstance(number, int)
        observed.add(kind)
        if kind == "small":
            assert number <= 2
        else:
            assert kind == "large"
            assert number >= 10
    assert observed == {"small", "large"}


@pytest.mark.parametrize(
    "schema",
    [
        {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "array",
            "items": [{"const": "head"}, {"type": "integer"}],
            "additionalItems": False,
            "minItems": 2,
        },
        {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "array",
            "items": {"type": "integer"},
            "prefixItems": [{"const": "ignored"}],
            "minItems": 1,
        },
        {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"a": {"const": 1}, "b": {"const": 2}},
            "required": ["a"],
            "dependencies": {"a": ["b"]},
            "additionalProperties": False,
        },
        {
            "$schema": "https://json-schema.org/draft/2019-09/schema",
            "type": "array",
            "items": {"type": "integer", "minimum": 0, "maximum": 10},
            "contains": {"minimum": 8},
            "minContains": 2,
            "maxContains": 2,
            "minItems": 3,
            "maxItems": 3,
        },
    ],
)
def test_selected_dialects_retain_their_own_keyword_semantics(schema: dict[str, JSONValue]) -> None:

    values = FixtureFactory.from_schema(schema).generate(count=20, seed=7)
    validator = validator_for(schema)(schema)
    assert len(values) == 20
    assert all(validator.is_valid(value) for value in values)


@pytest.mark.parametrize(
    "format_name",
    [
        "email",
        "idn-email",
        "hostname",
        "idn-hostname",
        "uri",
        "uri-reference",
        "iri",
        "iri-reference",
        "uri-template",
        "uuid",
        "date",
        "date-time",
        "time",
        "duration",
        "ipv4",
        "ipv6",
        "json-pointer",
        "relative-json-pointer",
        "regex",
    ],
)
def test_standard_formats_generate_values_checked_by_the_independent_oracle(
    format_name: str,
) -> None:
    schema: dict[str, JSONValue] = {"type": "string", "format": format_name}
    values = FixtureFactory.from_schema(schema).generate(count=10, seed=42)
    assert all(
        Draft202012Validator(schema, format_checker=Draft202012Validator.FORMAT_CHECKER).is_valid(
            value
        )
        for value in values
    )


def test_unknown_formats_are_annotations_while_other_constraints_remain_enforced() -> None:
    values = FixtureFactory.from_schema(
        {"type": "string", "format": "custom-annotation", "minLength": 2, "maxLength": 5}
    ).generate(count=10, seed=42)
    for value in values:
        assert isinstance(value, str)
        assert 2 <= len(value) <= 5


def test_typed_cases_retain_extra_payload_data_ignored_by_a_model() -> None:
    class Model(BaseModel):
        known: Literal["expected"]

    factory = FixtureFactory.from_schema(
        {"type": "object", "required": ["known"], "properties": {"known": {"const": "expected"}}}
    ).as_model(Model)
    observed_extra = False
    for case in factory.success_cases(count=30, seed=42):
        assert case.value.known == "expected"
        assert isinstance(case.payload, dict)
        assert case.payload["known"] == "expected"
        observed_extra = observed_extra or bool(set(case.payload) - {"known"})
    assert observed_extra


@pytest.mark.parametrize(
    ("dialect", "ignored"),
    [
        ("https://json-schema.org/draft/2020-12/schema", "dependencies"),
        ("http://json-schema.org/draft-07/schema#", "dependentRequired"),
        ("http://json-schema.org/draft-07/schema#", "dependentSchemas"),
    ],
)
def test_keywords_inactive_in_the_selected_dialect_do_not_add_generation_requirements(
    dialect: str, ignored: str
) -> None:
    schema: dict[str, JSONValue] = {
        "$schema": dialect,
        "type": "object",
        "required": ["a"],
        "properties": {"a": {"const": 1}},
        "additionalProperties": False,
    }
    schema[ignored] = {"a": {"required": ["b"]}} if ignored == "dependentSchemas" else {"a": ["b"]}
    assert FixtureFactory.from_schema(schema).generate(count=5, seed=42) == [{"a": 1}] * 5


@pytest.mark.parametrize("override", [False, True])
def test_openapi_document_dialect_is_inherited_and_schema_override_wins(*, override: bool) -> None:
    schema: dict[str, JSONValue] = {
        "type": "array",
        "items": [{"const": "head"}],
        "additionalItems": False,
        "minItems": 1,
    }
    if override:
        schema = {
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "array",
            "prefixItems": [{"const": "head"}],
            "items": False,
            "minItems": 1,
        }
    document: dict[str, JSONValue] = {
        "openapi": "3.1.0",
        "jsonSchemaDialect": "http://json-schema.org/draft-07/schema#",
        "paths": {
            "/record": {
                "get": {
                    "responses": {
                        "200": {
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/Body"}
                                }
                            }
                        }
                    }
                }
            }
        },
        "components": {"schemas": {"Body": schema}},
    }
    assert (
        FixtureFactory.from_openapi(document, path="/record").generate(count=5, seed=42)
        == [["head"]] * 5
    )


def test_openapi30_exclusive_bounds_and_nullable_are_preserved() -> None:
    document: dict[str, JSONValue] = {
        "openapi": "3.0.3",
        "paths": {
            "/record": {
                "get": {
                    "responses": {
                        "200": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "additionalProperties": False,
                                        "required": ["number", "text"],
                                        "properties": {
                                            "number": {
                                                "type": "integer",
                                                "minimum": 1,
                                                "exclusiveMinimum": True,
                                                "maximum": 3,
                                                "exclusiveMaximum": True,
                                            },
                                            "text": {"type": "string", "nullable": True},
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
    }
    observed_null = False
    observed_string = False
    for value in FixtureFactory.from_openapi(document, path="/record").generate(
        count=50, seed=42, profile="boundary"
    ):
        assert isinstance(value, dict)
        assert value["number"] == 2
        if value["text"] is None:
            observed_null = True
        else:
            assert isinstance(value["text"], str)
            observed_string = True
    assert observed_null
    assert observed_string


def test_2019_tuple_items_leave_unevaluated_tail_values_to_their_schema() -> None:
    schema: dict[str, JSONValue] = {
        "$schema": "https://json-schema.org/draft/2019-09/schema",
        "type": "array",
        "items": [{"const": "head"}],
        "minItems": 2,
        "maxItems": 2,
        "unevaluatedItems": {"const": 17},
    }
    assert FixtureFactory.from_schema(schema).generate(count=5, seed=42) == [["head", 17]] * 5


def test_allof_item_evaluation_does_not_close_an_otherwise_open_array() -> None:
    schema: dict[str, JSONValue] = {
        "type": "array",
        "minItems": 1,
        "allOf": [{"items": True}],
        "unevaluatedItems": False,
    }
    values = FixtureFactory.from_schema(schema).generate(count=10, seed=42)
    assert all(isinstance(value, list) and len(value) >= 1 for value in values)
    assert all(Draft202012Validator(schema).is_valid(value) for value in values)


def test_allof_contains_hints_generate_the_required_values() -> None:
    schema: dict[str, JSONValue] = {"type": "array", "allOf": [{"contains": {"const": 17}}]}
    for value in FixtureFactory.from_schema(schema).generate(count=10, seed=42):
        assert isinstance(value, list)
        assert 17 in value
        assert Draft202012Validator(schema).is_valid(value)
