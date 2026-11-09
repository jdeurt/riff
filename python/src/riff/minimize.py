"""Typed failure-oracle adaptation and minimization results."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import TypeAdapter

from riff.compiler import JSON_ADAPTER
from riff.types import JSONValue

if TYPE_CHECKING:
    from collections.abc import Callable

    from riff._native import NativeFactory


@dataclass(frozen=True)
class MinimizationResult:
    """Retained reproducer, oracle-call count, and budget exhaustion state."""

    value: JSONValue
    evaluations: int
    budget_exhausted: bool


RESULT = TypeAdapter[MinimizationResult](MinimizationResult)


def minimize(
    value: JSONValue,
    native: NativeFactory,
    *,
    is_failure: Callable[[JSONValue], bool],
    max_evaluations: int,
    preserve_schema: bool,
) -> MinimizationResult:
    """Reduce a reproducer while propagating caller-owned oracle exceptions."""

    def oracle(serialized: str) -> bool:
        return is_failure(JSON_ADAPTER.validate_json(serialized))

    return RESULT.validate_json(
        native.minimize(
            json.dumps(JSON_ADAPTER.validate_python(value), allow_nan=False),
            json.dumps({"maxEvaluations": max_evaluations, "preserveSchema": preserve_schema}),
            oracle,
        )
    )
