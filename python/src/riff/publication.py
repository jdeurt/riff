"""Publish OpenAPI copies without fixture annotations."""

import json

from pydantic import TypeAdapter

from riff._native import strip_metadata
from riff.types import JSONValue

DOCUMENT = TypeAdapter[dict[str, JSONValue]](dict[str, JSONValue])


def strip_fixture_metadata(document: dict[str, JSONValue]) -> dict[str, JSONValue]:
    """Remove schema annotations while preserving constraints, refs, and example data."""
    payload = DOCUMENT.validate_python(document)
    return DOCUMENT.validate_json(strip_metadata(json.dumps(payload, allow_nan=False)))
