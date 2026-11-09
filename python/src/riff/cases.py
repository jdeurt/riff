"""Label cases and replace one explicitly declared failure target."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from jsonschema import Draft7Validator, FormatChecker

from riff.compiler import JSON_ADAPTER
from riff.contracts import FailureCase, FixtureCase, SuccessCase, Violation
from riff.errors import GenerationError

if TYPE_CHECKING:
    from collections.abc import Iterator, Sequence

    from riff.compiler import FailureSource, Node, SchemaNode
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
        return [SuccessCase(value) for value in engine.generate(schema, count=count)]
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
        if Draft7Validator({"const": value}).is_valid(mutated):
            continue
        violations = tuple(
            Violation(_pointer(list(error.absolute_path)), error.message)
            for error in Draft7Validator(raw, format_checker=FormatChecker()).iter_errors(mutated)
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
        for name, child in schema.properties.items():
            yield from _failure_targets(child, value.get(name), (*tokens, name))
    if isinstance(value, list):
        for index, item in enumerate(value):
            yield from _failure_targets(schema.items, item, (*tokens, index))
    for branch in schema.all_of:
        yield from _failure_targets(branch, value, tokens)
    for branch in [*schema.one_of, *schema.any_of]:
        raw = branch if isinstance(branch, bool) else branch.contract
        if Draft7Validator(raw, format_checker=FormatChecker()).is_valid(value):
            yield from _failure_targets(branch, value, tokens)


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
