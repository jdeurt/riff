"""Typed oracle bridge for the shared native minimizer."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from riff.compiler import JSON_ADAPTER
from riff.documents import decode_native
from riff.errors import SchemaError
from riff.types import JSONValue

if TYPE_CHECKING:
    from collections.abc import Callable

    from riff._native import NativeFactory


@dataclass(frozen=True)
class MinimizationResult:
    """Native reproducer, oracle-call count, and budget exhaustion state."""

    value: JSONValue
    evaluations: int
    budget_exhausted: bool


class MinimizationOptions(BaseModel):
    """Validate the caller's minimizer controls before evaluating an oracle."""

    model_config = ConfigDict(strict=True)
    max_evaluations: int = Field(ge=1)
    preserve_schema: bool


RESULT = TypeAdapter[MinimizationResult](MinimizationResult)


def minimize(
    value: JSONValue,
    native: NativeFactory,
    *,
    is_failure: Callable[[JSONValue], bool],
    max_evaluations: int,
    preserve_schema: bool,
) -> MinimizationResult:
    """Reduce through Rust; exceptions from the caller's oracle propagate."""
    options = MinimizationOptions(max_evaluations=max_evaluations, preserve_schema=preserve_schema)
    payload = JSON_ADAPTER.validate_python(value)

    oracle_error: SchemaError | None = None

    def oracle(serialized: str) -> bool:
        nonlocal oracle_error
        try:
            return is_failure(decode_native(serialized))
        except SchemaError as error:
            oracle_error = error
            raise

    try:
        output = native.minimize(
            json.dumps(payload, allow_nan=False),
            options.max_evaluations,
            options.preserve_schema,
            oracle,
        )
    except SchemaError as error:
        if error is oracle_error:
            raise
        raise ValueError(str(error)) from error
    return RESULT.validate_python(decode_native(output))
