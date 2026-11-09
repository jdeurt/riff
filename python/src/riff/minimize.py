"""Greedy, budgeted reduction driven by a caller-owned failure oracle."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from jsonschema import Draft7Validator, FormatChecker
from pydantic import BaseModel, ConfigDict, Field

from riff.compiler import JSON_ADAPTER

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

    from riff.types import JSONValue, Schema


@dataclass(frozen=True)
class MinimizationResult:
    """A reduced reproducer and the number of failure-oracle evaluations."""

    value: JSONValue
    evaluations: int
    budget_exhausted: bool


class MinimizationOptions(BaseModel):
    """Validate minimizer inputs before evaluating the caller's oracle."""

    model_config = ConfigDict(strict=True)
    max_evaluations: int = Field(ge=1)
    preserve_schema: bool


def minimize(
    value: JSONValue,
    schema: Schema,
    *,
    is_failure: Callable[[JSONValue], bool],
    max_evaluations: int,
    preserve_schema: bool,
) -> MinimizationResult:
    """Keep reductions that still fail; schema preservation is caller-controlled."""
    options = MinimizationOptions(max_evaluations=max_evaluations, preserve_schema=preserve_schema)
    current = JSON_ADAPTER.validate_python(value)
    validator = Draft7Validator(schema, format_checker=FormatChecker())
    if preserve_schema and not validator.is_valid(current):
        message = (
            "Initial value violates the schema; use preserve_schema=False for malformed cases."
        )
        raise ValueError(message)
    if not is_failure(deepcopy(current)):
        message = "Initial value does not reproduce the failure."
        raise ValueError(message)
    evaluations = 1
    while evaluations < options.max_evaluations:
        for candidate in _reductions(current):
            if Draft7Validator({"const": current}).is_valid(candidate):
                continue
            if preserve_schema and not validator.is_valid(candidate):
                continue
            evaluations += 1
            if is_failure(deepcopy(candidate)):
                current = candidate
                break
            if evaluations >= options.max_evaluations:
                return MinimizationResult(current, evaluations, budget_exhausted=True)
        else:
            return MinimizationResult(current, evaluations, budget_exhausted=False)
    return MinimizationResult(current, evaluations, budget_exhausted=True)


def _reductions(value: JSONValue) -> Iterator[JSONValue]:
    if isinstance(value, bool):
        if value:
            yield False
    elif isinstance(value, (int, float)):
        yield from _number_reductions(value)
    elif isinstance(value, str) and value:
        yield ""
        yield value[: len(value) // 2]
        yield value[len(value) // 2 :]
        yield value[:-1]
        yield value[1:]
    elif isinstance(value, list) and value:
        yield []
        for index, child in enumerate(value):
            yield value[:index] + value[index + 1 :]
            for reduced in _reductions(child):
                yield [*value[:index], reduced, *value[index + 1 :]]
    elif isinstance(value, dict) and value:
        yield from _object_reductions(value)


def _object_reductions(value: dict[str, JSONValue]) -> Iterator[JSONValue]:
    yield {}
    for name, child in value.items():
        yield {key: item for key, item in value.items() if key != name}
        for reduced in _reductions(child):
            yield {**value, name: reduced}


def _number_reductions(value: float) -> Iterator[JSONValue]:
    if value == 0:
        return
    yield 0
    half = value // 2 if isinstance(value, int) else value / 2
    if abs(half) < abs(value):
        yield half
    toward_zero = value - 1 if value > 0 else value + 1
    if abs(toward_zero) < abs(value):
        yield toward_zero
