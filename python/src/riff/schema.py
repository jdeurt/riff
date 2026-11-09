"""Resolve local contracts and normalize the supported JSON Schema dialect."""

from __future__ import annotations

from typing import TYPE_CHECKING

from jsonschema import Draft7Validator
from jsonschema.exceptions import SchemaError as ValidationSchemaError

from riff.errors import SchemaError

if TYPE_CHECKING:
    from riff.types import JSONValue, Schema

SCHEMA_MAPS = frozenset({"properties", "patternProperties"})
SCHEMA_LISTS = frozenset({"allOf", "anyOf", "oneOf"})
SCHEMA_CHILDREN = frozenset({"items", "additionalProperties", "not", "contains"})
SUPPORTED_KEYWORDS = frozenset(
    {
        "$schema",
        "$id",
        "$ref",
        "$defs",
        "definitions",
        "title",
        "description",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
        "xml",
        "externalDocs",
        "discriminator",
        "type",
        "enum",
        "const",
        "format",
        "pattern",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "properties",
        "required",
        "additionalProperties",
        "patternProperties",
        "minProperties",
        "maxProperties",
        "items",
        "minItems",
        "maxItems",
        "uniqueItems",
        "allOf",
        "anyOf",
        "oneOf",
        "not",
        "contains",
        "nullable",
    }
)
SUPPORTED_DIALECTS = frozenset(
    {"http://json-schema.org/draft-07/schema#", "https://json-schema.org/draft-07/schema"}
)


def resolve_pointer(document: JSONValue, reference: str) -> JSONValue:
    """Resolve a local JSON Pointer without reading files or making requests."""
    if reference == "#":
        return document
    if not reference.startswith("#/"):
        message = f"Only local JSON Pointer references are supported: {reference}"
        raise SchemaError(message)
    current = document
    for token in reference[2:].split("/"):
        key = token.replace("~1", "/").replace("~0", "~")
        if not isinstance(current, dict) or key not in current:
            message = f"Unresolved reference: {reference}"
            raise SchemaError(message)
        current = current[key]
    return current


def dereference_object(document: JSONValue, value: JSONValue) -> dict[str, JSONValue]:
    """Follow references on OpenAPI response objects, rejecting reference cycles."""
    seen: set[str] = set()
    while isinstance(value, dict) and "$ref" in value:
        reference = value["$ref"]
        if not isinstance(reference, str) or reference in seen:
            message = "Invalid or recursive OpenAPI object reference."
            raise SchemaError(message)
        seen.add(reference)
        value = resolve_pointer(document, reference)
    if not isinstance(value, dict):
        message = "Expected an OpenAPI object."
        raise SchemaError(message)
    return value


def normalize_schema(schema: JSONValue, document: JSONValue, *, openapi30: bool) -> Schema:
    """Inline reachable references, normalize OpenAPI 3.0 bounds, and validate."""
    normalized = _normalize(schema, document, openapi30=openapi30, references=())
    try:
        Draft7Validator.check_schema(normalized)
    except ValidationSchemaError as error:
        message = f"Invalid schema: {error.message}"
        raise SchemaError(message) from error
    return normalized


def _normalize(
    schema: JSONValue,
    document: JSONValue,
    *,
    openapi30: bool,
    references: tuple[str, ...],
) -> Schema:
    if isinstance(schema, bool):
        return schema
    if not isinstance(schema, dict):
        message = "Each schema must be an object or boolean."
        raise SchemaError(message)
    if "$ref" in schema:
        return _normalize_reference(schema, document, openapi30=openapi30, references=references)
    result: dict[str, JSONValue] = {}
    for key, value in schema.items():
        if key in {"$defs", "definitions", "$id"}:
            continue
        if key == "$schema":
            _validate_dialect(value)
            continue
        if key not in SUPPORTED_KEYWORDS and not key.startswith("x-"):
            message = f"Unsupported schema keyword: {key}"
            raise SchemaError(message)
        result[key] = _normalize_children(
            key, value, document, openapi30=openapi30, references=references
        )
    if openapi30:
        _normalize_openapi30(result)
    elif "nullable" in result:
        message = "nullable is an OpenAPI 3.0 keyword; use a null type in other schemas."
        raise SchemaError(message)
    return result


def _validate_dialect(value: JSONValue) -> None:
    if not isinstance(value, str) or value not in SUPPORTED_DIALECTS:
        message = "Standalone schemas must use the supported Draft 7 subset."
        raise SchemaError(message)


def _normalize_reference(
    schema: dict[str, JSONValue],
    document: JSONValue,
    *,
    openapi30: bool,
    references: tuple[str, ...],
) -> Schema:
    reference = schema["$ref"]
    if not isinstance(reference, str) or reference in references:
        message = f"Invalid or recursive schema reference: {reference}"
        raise SchemaError(message)
    siblings = {key: value for key, value in schema.items() if key != "$ref"}
    if siblings:
        message = f"$ref siblings are unsupported; use allOf around {reference}."
        raise SchemaError(message)
    target = resolve_pointer(document, reference)
    return _normalize(target, document, openapi30=openapi30, references=(*references, reference))


def _normalize_children(
    key: str,
    value: JSONValue,
    document: JSONValue,
    *,
    openapi30: bool,
    references: tuple[str, ...],
) -> JSONValue:
    if key in SCHEMA_MAPS and isinstance(value, dict):
        return {
            name: _normalize(child, document, openapi30=openapi30, references=references)
            for name, child in value.items()
        }
    if key in SCHEMA_LISTS and isinstance(value, list):
        return [
            _normalize(child, document, openapi30=openapi30, references=references)
            for child in value
        ]
    if key in SCHEMA_CHILDREN:
        return _normalize(value, document, openapi30=openapi30, references=references)
    return value


def _normalize_openapi30(schema: dict[str, JSONValue]) -> None:
    for bound in ("Minimum", "Maximum"):
        exclusive = schema.pop(f"exclusive{bound}", False)
        if not isinstance(exclusive, bool):
            message = "OpenAPI 3.0 exclusive bounds must be booleans."
            raise SchemaError(message)
        inclusive_key = bound.lower()
        if exclusive:
            if inclusive_key not in schema:
                message = f"exclusive{bound} requires {inclusive_key}."
                raise SchemaError(message)
            schema[f"exclusive{bound}"] = schema.pop(inclusive_key)
    nullable = schema.pop("nullable", False)
    if not isinstance(nullable, bool):
        message = "nullable must be a boolean."
        raise SchemaError(message)
    if nullable and isinstance(schema.get("type"), str):
        schema["type"] = [schema["type"], "null"]
