"""Remove fixture annotations from a public OpenAPI copy while preserving data."""

from __future__ import annotations

from copy import deepcopy
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from riff.types import JSONValue

SCHEMA_MAPS = frozenset(
    {"properties", "patternProperties", "$defs", "definitions", "dependentSchemas", "dependencies"}
)
SCHEMA_LISTS = frozenset({"allOf", "anyOf", "oneOf", "prefixItems"})
SCHEMA_CHILDREN = frozenset(
    {
        "items",
        "additionalItems",
        "additionalProperties",
        "unevaluatedProperties",
        "unevaluatedItems",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "propertyNames",
        "contentSchema",
    }
)


def strip_fixture_metadata(document: dict[str, JSONValue]) -> dict[str, JSONValue]:
    """Return a clean OpenAPI copy, retaining constraints, refs, and example data.

    Schema annotation keys are removed; payload properties literally named
    x-fixture and values inside example/default data remain intact.
    """
    result: dict[str, JSONValue] = {}
    for key, value in document.items():
        if key == "schema":
            result[key] = _strip_schema(value)
        elif key == "schemas" and isinstance(value, dict):
            result[key] = {name: _strip_schema(child) for name, child in value.items()}
        elif key in {"example", "examples", "default"} or key.startswith("x-"):
            result[key] = deepcopy(value)
        else:
            result[key] = _walk_openapi(value)
    return result


def _walk_openapi(value: JSONValue) -> JSONValue:
    if isinstance(value, dict):
        return strip_fixture_metadata(value)
    if isinstance(value, list):
        return [_walk_openapi(child) for child in value]
    return value


def _strip_schema(value: JSONValue) -> JSONValue:
    if not isinstance(value, dict):
        return deepcopy(value)
    result: dict[str, JSONValue] = {}
    for key, child in value.items():
        if key == "x-fixture":
            continue
        if key in SCHEMA_MAPS and isinstance(child, dict):
            result[key] = {name: _strip_schema(schema) for name, schema in child.items()}
        elif key in SCHEMA_LISTS and isinstance(child, list):
            result[key] = [_strip_schema(schema) for schema in child]
        elif key in SCHEMA_CHILDREN:
            result[key] = (
                [_strip_schema(schema) for schema in child]
                if isinstance(child, list)
                else _strip_schema(child)
            )
        else:
            result[key] = deepcopy(child)
    return result
