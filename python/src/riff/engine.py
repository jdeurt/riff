"""Bounded generation over compiled, concretely typed schemas and strategies."""

from __future__ import annotations

import math
import string
from copy import deepcopy
from decimal import ROUND_CEILING, ROUND_FLOOR, Decimal
from random import Random
from typing import TYPE_CHECKING

from jsonschema import Draft7Validator, FormatChecker
from pydantic import ValidationError
from rstr import Rstr

from riff.compiler import (
    JSON_ADAPTER,
    ChoiceStrategy,
    FilterStrategy,
    FixtureMetadata,
    Node,
    ProviderStrategy,
    SchemaNode,
    SchemaStrategy,
    Source,
    Strategy,
    ValuesStrategy,
    intersect_nodes,
)
from riff.contracts import Provider, ProviderContext
from riff.errors import GenerationError, ProviderError
from riff.mutations import violation_candidates
from riff.providers import formatted_value, make_faker, semantic_value

if TYPE_CHECKING:
    from collections.abc import Mapping

    from faker import Faker

    from riff.types import JSONValue, Profile

MAX_DEPTH = 32
MAX_SIZE = 1024
STRING_EDGES = ("", " ", "\t\n", "é", "e\u0301", "日本語", "🦊", "\u0000", "'\"\\", "<>&")


class Engine:
    """Own randomness, provider instances, and budgets for one batch."""

    def __init__(
        self, *, seed: int, profile: Profile, max_attempts: int, providers: Mapping[str, Provider]
    ) -> None:
        """Accept validated generation options and a snapshotted provider registry."""
        self.random = Random(seed)  # noqa: S311 -- Test data is not security-sensitive.
        self.profile = profile
        self.max_attempts = max_attempts
        self.providers = providers
        self.fakers: dict[str, Faker] = {}
        self.regex = Rstr(self.random)
        self.remaining_candidates = 0

    def generate(self, schema: Node, *, count: int) -> list[JSONValue]:
        """Generate a complete valid batch, with no partial return on failure."""
        fixtures: list[JSONValue] = []
        for _ in range(count):
            self.remaining_candidates = self.max_attempts * 100
            try:
                fixtures.append(self._validated_value(schema, depth=0))
            except (ArithmeticError, ValueError, KeyError, NotImplementedError) as error:
                message = f"Unable to generate the requested constraints: {error}"
                raise GenerationError(message) from error
        return fixtures

    def provider_value(
        self, *, name: str, locale: str, options: dict[str, JSONValue], schema: SchemaNode
    ) -> JSONValue:
        """Invoke a registered provider without exposing backend-specific context."""
        if name not in self.providers:
            return semantic_value(name, self._faker(locale))
        context = ProviderContext(
            seed=self.random.getrandbits(64),
            locale=locale,
            schema=deepcopy(schema.contract),
            options=deepcopy(options),
        )
        try:
            return JSON_ADAPTER.validate_python(self.providers[name](context))
        except Exception as error:
            # Isolate arbitrary user code and invalid returns behind a public error.
            message = f"Custom provider {name!r} failed: {error}"
            raise ProviderError(message) from error

    def source_value(self, source: Source, schema: SchemaNode) -> JSONValue:
        """Evaluate an explicit source, including deliberately negative sources."""
        if source.strategy is not None:
            return self._strategy_value(source.strategy, schema, depth=0)
        if source.examples is not None and (
            source.provider is None or self.random.randrange(2) == 0
        ):
            return deepcopy(self.random.choice(source.examples))
        if source.provider is None:
            message = "Source does not declare a provider or explicit values."
            raise GenerationError(message)
        return self.provider_value(
            name=source.provider, locale=source.locale, options=source.options, schema=schema
        )

    def _spend_candidate(self, *, depth: int) -> None:
        self.remaining_candidates -= 1
        if self.remaining_candidates < 0 or depth > MAX_DEPTH:
            message = "Generation exceeded its candidate or depth safety budget."
            raise GenerationError(message)

    def _validated_value(self, schema: Node, *, depth: int) -> JSONValue:
        raw = schema if isinstance(schema, bool) else schema.contract
        validator = Draft7Validator(raw, format_checker=FormatChecker())
        last_error = "no candidate satisfies the schema"
        for _ in range(self.max_attempts):
            try:
                candidate = JSON_ADAPTER.validate_python(self._candidate(schema, depth=depth))
            except GenerationError as error:
                if self.remaining_candidates <= 0:
                    raise
                last_error = str(error)
                continue
            except ValidationError as error:
                last_error = str(error)
                continue
            error = next(iter(validator.iter_errors(candidate)), None)
            if error is None:
                return candidate
            last_error = error.message
        message = (
            f"No valid fixture after {self.max_attempts} attempts at depth {depth}: {last_error}"
        )
        raise GenerationError(message)

    def _candidate(self, schema: Node, *, depth: int) -> JSONValue:
        self._spend_candidate(depth=depth)
        if schema is False:
            message = "A false schema has no valid fixtures."
            raise GenerationError(message)
        if schema is True:
            return self.random.choice([None, False, 0, ""])
        return self._schema_candidate(schema, depth=depth)

    def _schema_candidate(self, schema: SchemaNode, *, depth: int) -> JSONValue:
        source = schema.fixture
        if source.strategy is not None:
            return self._strategy_value(source.strategy, schema, depth=depth)
        if source.examples is not None and self.random.randrange(3) == 0:
            return deepcopy(self.random.choice(source.examples))
        if "const" in schema.model_fields_set:
            return deepcopy(schema.const)
        if schema.enum is not None:
            return deepcopy(self.random.choice(schema.enum))
        use_provider = source.provider is not None and (
            self.profile == "realistic"
            or (self.profile == "mixed" and self.random.randrange(2) == 0)
        )
        if use_provider:
            return self.source_value(source, schema)
        return self._structural_value(schema, depth=depth)

    def _structural_value(self, schema: SchemaNode, *, depth: int) -> JSONValue:
        if schema.all_of:
            hint: Node = schema.model_copy(deep=True)
            hint.all_of = []
            for branch in schema.all_of:
                hint = intersect_nodes(hint, branch)
            return self._candidate(hint, depth=depth + 1)
        alternatives = schema.one_of or schema.any_of
        if alternatives:
            siblings = schema.model_copy(deep=True)
            siblings.one_of = []
            siblings.any_of = []
            hint = intersect_nodes(siblings, self.random.choice(alternatives))
            return self._candidate(hint, depth=depth + 1)
        return self._typed_value(schema, depth=depth)

    def _typed_value(self, schema: SchemaNode, *, depth: int) -> JSONValue:
        field_type = self._choose_type(schema)
        match field_type:
            case "null":
                return None
            case "boolean":
                return bool(self.random.randrange(2))
            case "integer" | "number":
                return self._number(schema, integer=field_type == "integer")
            case "string":
                return self._string(schema)
            case "array":
                return self._array(schema, depth=depth)
            case "object":
                return self._object(schema, depth=depth)
            case _:
                message = f"Unsupported generation type: {field_type}"
                raise GenerationError(message)

    def _choose_type(self, schema: SchemaNode) -> str:
        if isinstance(schema.types, str):
            return schema.types
        if isinstance(schema.types, list):
            return self.random.choice(schema.types)
        if schema.properties or schema.required:
            return "object"
        if "items" in schema.model_fields_set:
            return "array"
        return self.random.choice(["null", "boolean", "integer", "string", "array", "object"])

    def _strategy_value(self, strategy: Strategy, schema: SchemaNode, *, depth: int) -> JSONValue:
        self._spend_candidate(depth=depth)
        if isinstance(strategy, SchemaStrategy):
            if strategy.constraints == "violate":
                return self.random.choice(violation_candidates(schema))
            structural = schema.model_copy(deep=True)
            structural.fixture = FixtureMetadata()
            return self._validated_value(structural, depth=depth + 1)
        if isinstance(strategy, ValuesStrategy):
            return deepcopy(self.random.choice(strategy.values))
        if isinstance(strategy, ProviderStrategy):
            return self.provider_value(
                name=strategy.name, locale=strategy.locale, options=strategy.options, schema=schema
            )
        if isinstance(strategy, ChoiceStrategy):
            return self._strategy_value(
                self.random.choice(strategy.strategies), schema, depth=depth + 1
            )
        return self._filtered_value(strategy, schema, depth=depth)

    def _filtered_value(
        self, strategy: FilterStrategy, schema: SchemaNode, *, depth: int
    ) -> JSONValue:
        predicate = (
            strategy.predicate
            if isinstance(strategy.predicate, bool)
            else strategy.predicate.contract
        )
        validator = Draft7Validator(predicate, format_checker=FormatChecker())
        for _ in range(self.max_attempts):
            value = self._strategy_value(strategy.strategy, schema, depth=depth + 1)
            if validator.is_valid(value):
                return value
        message = "Strategy filter exhausted its rejection budget."
        raise GenerationError(message)

    def _size(self, *, minimum: int, maximum: int | None, default: int) -> int:
        upper = min(maximum if maximum is not None else max(minimum, default), MAX_SIZE)
        if minimum > upper:
            message = f"Inconsistent size bounds or minimum exceeds safety limit {MAX_SIZE}."
            raise GenerationError(message)
        if self.profile == "boundary" or self.random.randrange(3) == 0:
            return self.random.choice([minimum, upper])
        return self.random.randint(minimum, upper)

    def _faker(self, locale: str = "en-US") -> Faker:
        if locale not in self.fakers:
            self.fakers[locale] = make_faker(locale, self.random)
        return self.fakers[locale]

    def _string(self, schema: SchemaNode) -> str:
        if schema.format is not None and schema.format not in {"int32", "int64", "float", "double"}:
            return formatted_value(schema.format, self._faker(), self.random)
        if schema.pattern is not None:
            value = self.regex.xeger(schema.pattern)
            if len(value) > MAX_SIZE:
                message = "Generated pattern exceeds the string safety limit."
                raise GenerationError(message)
            return value
        if self.profile != "realistic" and self.random.randrange(2) == 0:
            return self.random.choice(STRING_EDGES)
        length = self._size(minimum=schema.min_length, maximum=schema.max_length, default=32)
        alphabet = string.ascii_letters + string.digits + " é日本🦊\t\n"
        return "".join(self.random.choices(alphabet, k=length))

    def _number(self, schema: SchemaNode, *, integer: bool) -> int | float:
        lower = Decimal(
            str(
                schema.minimum
                if schema.minimum is not None
                else schema.exclusive_minimum
                if schema.exclusive_minimum is not None
                else -1000
            )
        )
        upper = Decimal(
            str(
                schema.maximum
                if schema.maximum is not None
                else schema.exclusive_maximum
                if schema.exclusive_maximum is not None
                else 1000
            )
        )
        if schema.minimum is None and schema.exclusive_minimum is None:
            lower = min(lower, upper - 1000)
        if schema.maximum is None and schema.exclusive_maximum is None:
            upper = max(upper, lower + 1000)
        if lower == upper and not integer and schema.multiple_of is None:
            return float(lower)
        default_step = Decimal(1) if integer else min(Decimal("0.01"), abs(upper - lower) / 100)
        multiple = (
            Decimal(str(schema.multiple_of)) if schema.multiple_of is not None else default_step
        )
        if integer:
            multiple = Decimal(multiple.as_integer_ratio()[0])
        first = int((lower / multiple).to_integral_value(rounding=ROUND_CEILING))
        last = int((upper / multiple).to_integral_value(rounding=ROUND_FLOOR))
        if schema.exclusive_minimum is not None:
            first = max(first, math.floor(Decimal(str(schema.exclusive_minimum)) / multiple) + 1)
        if schema.exclusive_maximum is not None:
            last = min(last, math.ceil(Decimal(str(schema.exclusive_maximum)) / multiple) - 1)
        if first > last:
            message = "Numeric bounds contain no supported multiple."
            raise GenerationError(message)
        if self.profile == "boundary" or self.random.randrange(3) == 0:
            index = self.random.choice([first, last, max(first, min(last, 0))])
        else:
            index = self.random.randint(first, last)
        value = multiple * index
        return int(value) if integer else float(value)

    def _array(self, schema: SchemaNode, *, depth: int) -> list[JSONValue]:
        size = self._size(minimum=schema.min_items, maximum=schema.max_items, default=5)
        values: list[JSONValue] = []
        for _ in range(size):
            for _ in range(self.max_attempts):
                value = self._validated_value(schema.items, depth=depth + 1)
                if not schema.unique_items or not any(
                    Draft7Validator({"const": previous}).is_valid(value) for previous in values
                ):
                    values.append(value)
                    break
            else:
                message = "Unable to generate enough distinct array items."
                raise GenerationError(message)
        return values

    def _object(self, schema: SchemaNode, *, depth: int) -> dict[str, JSONValue]:
        selected = list(dict.fromkeys(schema.required))
        optional = [
            name
            for name, child in schema.properties.items()
            if name not in selected and child is not False
        ]
        self.random.shuffle(optional)
        minimum = max(schema.min_properties, len(selected))
        default_maximum = max(len(schema.properties), minimum)
        wanted = self._size(minimum=minimum, maximum=schema.max_properties, default=default_maximum)
        selected.extend(optional[: max(0, wanted - len(selected))])
        values = {
            name: self._validated_value(
                schema.properties.get(name, schema.additional), depth=depth + 1
            )
            for name in selected
        }
        if schema.additional is False:
            return values
        for index in range(len(schema.properties) + wanted):
            if len(values) >= wanted:
                break
            self._spend_candidate(depth=depth)
            name = f"extra_{index}"
            if name not in values and name not in schema.properties:
                values[name] = self._validated_value(schema.additional, depth=depth + 1)
        return values
