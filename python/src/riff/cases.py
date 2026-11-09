"""Label cases and replace one explicitly declared failure target."""

from __future__ import annotations

import re
from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from riff.compiler import JSON_ADAPTER, SchemaNode, accepts_value, applicable_branches
from riff.contracts import FailureCase, FixtureCase, SuccessCase, Violation
from riff.errors import GenerationError
from riff.validation import schema_validator

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

    from riff.compiler import FailureSource, Node
    from riff.engine import Engine
    from riff.types import Expectation, JSONValue


@dataclass(frozen=True)
class FailureTarget:
    """A reachable typed field with its own declared negative data source."""

    tokens: tuple[str | int, ...]
    schema: SchemaNode
    source: FailureSource


def generate_cases(
    schema: Node, engine: Engine, *, count: int, expectation: Expectation
) -> list[FixtureCase]:
    """Generate labels supplied by the caller, independently of schema validity."""
    if expectation == "pass":
        return [
            SuccessCase(value=value, payload=value)
            for value in engine.generate(schema, count=count)
        ]
    if expectation == "fail":
        return list(generate_failure_cases(schema, engine, count=count))
    message = f"Unknown case expectation: {expectation}"
    raise ValueError(message)


def generate_failure_cases(schema: Node, engine: Engine, *, count: int) -> list[FailureCase]:
    """Return a complete batch of explicitly changed negative fixtures."""
    return [_failure_case(schema, engine) for _ in range(count)]


def _failure_case(schema: Node, engine: Engine) -> FailureCase:
    raw = schema if isinstance(schema, bool) else schema.contract
    for _ in range(engine.max_attempts):
        value = engine.generate(schema, count=1)[0]
        targets = list(_failure_targets(schema, value, ()))
        if not targets:
            continue
        target = engine.random.choice(targets)
        engine.remaining_candidates = engine.max_attempts * 100
        try:
            replacement = JSON_ADAPTER.validate_python(
                engine.source_value(target.source, target.schema)
            )
        except (ArithmeticError, ValueError, NotImplementedError) as error:
            message = f"Unable to generate a JSON failure replacement: {error}"
            raise GenerationError(message) from error
        mutated = _replace(value, target.tokens, replacement)
        if schema_validator({"const": value}).is_valid(mutated):
            continue
        violations = tuple(
            Violation(_pointer(list(error.absolute_path)), error.message)
            for error in schema_validator(raw).iter_errors(mutated)
        )
        return FailureCase(
            value=mutated,
            schema_valid=not violations,
            violations=violations,
            target=_pointer(target.tokens),
            reason=target.source.reason,
        )
    message = (
        "No reachable failure declaration produced a changed fixture within the attempt budget."
    )
    raise GenerationError(message)


def _failure_targets(
    schema: Node, value: JSONValue, tokens: tuple[str | int, ...]
) -> Iterator[FailureTarget]:
    if isinstance(schema, bool):
        return
    if schema.fixture.fail is not None:
        yield FailureTarget(tokens, schema, schema.fixture.fail)
    if isinstance(value, dict):
        yield from _object_failure_targets(schema, value, tokens)
    if isinstance(value, list):
        yield from _array_failure_targets(schema, value, tokens)
    for branch in applicable_branches(schema, value):
        yield from _failure_targets(branch, value, tokens)


def _object_failure_targets(
    schema: SchemaNode, value: dict[str, JSONValue], tokens: tuple[str | int, ...]
) -> Iterator[FailureTarget]:
    for name, child in schema.properties.items():
        yield from _failure_targets(child, value.get(name), (*tokens, name))
    for name, item in value.items():
        matches = [child for pattern, child in schema.patterns.items() if re.search(pattern, name)]
        for child in matches:
            yield from _failure_targets(child, item, (*tokens, name))
        if name not in schema.properties and not matches:
            yield from _failure_targets(schema.additional, item, (*tokens, name))
    if schema.unevaluated_properties is not None:
        evaluated = _evaluated_keys(schema, value, include_unevaluated=False)
        for name in value.keys() - evaluated:
            yield from _failure_targets(schema.unevaluated_properties, value[name], (*tokens, name))


def _array_failure_targets(
    schema: SchemaNode, value: list[JSONValue], tokens: tuple[str | int, ...]
) -> Iterator[FailureTarget]:
    evaluated_items = _evaluated_items(schema, value, include_unevaluated=False)
    for index, item in enumerate(value):
        # Use the declared item schema; unevaluated annotations are applied below.
        declared = schema.model_copy()
        declared.unevaluated_items = None
        yield from _failure_targets(declared.item_schema(index), item, (*tokens, index))
        if schema.contains is not None and accepts_value(schema.contains, item):
            yield from _failure_targets(schema.contains, item, (*tokens, index))
        if schema.unevaluated_items is not None and index not in evaluated_items:
            yield from _failure_targets(schema.unevaluated_items, item, (*tokens, index))


def _evaluated_keys(
    schema: SchemaNode, value: dict[str, JSONValue], *, include_unevaluated: bool
) -> set[str]:
    evaluated = set(schema.properties).intersection(value)
    evaluated.update(
        name for name in value for pattern in schema.patterns if re.search(pattern, name)
    )
    if "additionalProperties" in schema.contract or (
        include_unevaluated and schema.unevaluated_properties is not None
    ):
        return set(value)
    for branch in applicable_branches(schema, value):
        if isinstance(branch, SchemaNode):
            evaluated.update(_evaluated_keys(branch, value, include_unevaluated=True))
    return evaluated


def _evaluated_items(
    schema: SchemaNode, value: list[JSONValue], *, include_unevaluated: bool
) -> set[int]:
    if ("items" in schema.contract and not isinstance(schema.items, list)) or (
        isinstance(schema.items, list) and "additionalItems" in schema.contract
    ):
        return set(range(len(value)))
    if include_unevaluated and schema.unevaluated_items is not None:
        return set(range(len(value)))
    prefix = schema.items if isinstance(schema.items, list) else schema.prefix_items
    evaluated = set(range(min(len(value), len(prefix))))
    if schema.contains is not None:
        evaluated.update(
            index for index, item in enumerate(value) if accepts_value(schema.contains, item)
        )
    for branch in applicable_branches(schema, value):
        if isinstance(branch, SchemaNode):
            evaluated.update(_evaluated_items(branch, value, include_unevaluated=True))
    return evaluated


def _replace(value: JSONValue, tokens: tuple[str | int, ...], replacement: JSONValue) -> JSONValue:
    if not tokens:
        return replacement
    first, *remaining = tokens
    if isinstance(value, dict) and isinstance(first, str):
        return {**value, first: _replace(value.get(first), tuple(remaining), replacement)}
    if isinstance(value, list) and isinstance(first, int):
        result = deepcopy(value)
        result[first] = _replace(value[first], tuple(remaining), replacement)
        return result
    message = "Failure target is not reachable in the generated body."
    raise GenerationError(message)


def _pointer(tokens: Sequence[str | int]) -> str:
    return "".join(f"/{str(token).replace('~', '~0').replace('/', '~1')}" for token in tokens)
