"""Construct deliberate constraint violations at schema boundaries."""

from __future__ import annotations

import math
from typing import TYPE_CHECKING

from riff.errors import GenerationError
from riff.validation import schema_validator

if TYPE_CHECKING:
    from riff.compiler import SchemaNode
    from riff.types import JSONValue

MAX_MUTATION_SIZE = 1024


def violation_candidates(schema: SchemaNode) -> list[JSONValue]:
    """Prefer same-type boundary violations, with type violations as a fallback."""
    validator = schema_validator(schema.contract, dialect=schema.dialect)
    boundaries = _numeric_boundaries(schema) + _structural_boundaries(schema)
    candidates = [
        value
        for value in boundaries
        if (not isinstance(value, float) or math.isfinite(value)) and not validator.is_valid(value)
    ]
    if candidates:
        return candidates
    fallbacks: list[JSONValue] = [None, False, 0, "", [], {}]
    candidates = [value for value in fallbacks if not validator.is_valid(value)]
    if candidates:
        return candidates
    message = "No supported constraint violation exists for the annotated schema."
    raise GenerationError(message)


def _numeric_boundaries(schema: SchemaNode) -> list[JSONValue]:
    candidates: list[JSONValue] = []
    integer = schema.types == "integer" or (
        isinstance(schema.types, list)
        and "integer" in schema.types
        and "number" not in schema.types
    )
    if schema.minimum is not None:
        candidates.append(
            math.ceil(schema.minimum) - 1 if integer else math.nextafter(schema.minimum, -math.inf)
        )
    if schema.maximum is not None:
        candidates.append(
            math.floor(schema.maximum) + 1 if integer else math.nextafter(schema.maximum, math.inf)
        )
    if schema.exclusive_minimum is not None:
        candidates.append(
            math.floor(schema.exclusive_minimum) if integer else schema.exclusive_minimum
        )
    if schema.exclusive_maximum is not None:
        candidates.append(
            math.ceil(schema.exclusive_maximum) if integer else schema.exclusive_maximum
        )
    return candidates


def _structural_boundaries(schema: SchemaNode) -> list[JSONValue]:
    candidates: list[JSONValue] = []
    if schema.min_length > 0:
        candidates.append("")
    if schema.max_length is not None and schema.max_length < MAX_MUTATION_SIZE:
        candidates.append("x" * (schema.max_length + 1))
    if schema.min_items > 0:
        candidates.append([])
    if schema.max_items is not None and schema.max_items < MAX_MUTATION_SIZE:
        candidates.append([None] * (schema.max_items + 1))
    if schema.required or schema.min_properties > 0:
        candidates.append({})
    if schema.additional is False:
        name = "extra_invalid"
        while name in schema.properties:
            name += "_"
        candidates.append({name: None})
    return candidates
