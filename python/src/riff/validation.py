"""Select JSON Schema validation semantics without weakening declared constraints."""

from __future__ import annotations

from typing import TYPE_CHECKING

from jsonschema import Draft7Validator, Draft201909Validator, Draft202012Validator

if TYPE_CHECKING:
    from jsonschema.protocols import Validator

    from riff.types import Schema

DEFAULT_DIALECT = "https://json-schema.org/draft/2020-12/schema"
DRAFT7_DIALECTS = frozenset(
    {"http://json-schema.org/draft-07/schema", "https://json-schema.org/draft-07/schema"}
)
SUPPORTED_DIALECTS = DRAFT7_DIALECTS | {
    DEFAULT_DIALECT,
    "https://json-schema.org/draft/2019-09/schema",
    "https://spec.openapis.org/oas/3.1/dialect/base",
}


def schema_validator(schema: Schema, *, dialect: str = DEFAULT_DIALECT) -> Validator:
    """Use the declared dialect, or inherited semantics for a nested schema."""
    declared = schema.get("$schema") if isinstance(schema, dict) else None
    selected = declared if isinstance(declared, str) else dialect
    selected = selected.rstrip("#")
    if selected in DRAFT7_DIALECTS:
        return Draft7Validator(schema, format_checker=Draft7Validator.FORMAT_CHECKER)
    if selected == "https://json-schema.org/draft/2019-09/schema":
        return Draft201909Validator(schema, format_checker=Draft201909Validator.FORMAT_CHECKER)
    return Draft202012Validator(schema, format_checker=Draft202012Validator.FORMAT_CHECKER)
