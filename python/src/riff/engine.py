"""Bounded generation over compiled, concretely typed schemas and strategies."""

from __future__ import annotations

import math
import re
import string
from copy import deepcopy
from decimal import ROUND_CEILING, ROUND_FLOOR, Decimal
from random import Random
from typing import TYPE_CHECKING

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
    negate_node,
)
from riff.contracts import Provider, ProviderContext
from riff.errors import GenerationError, ProviderError
from riff.mutations import violation_candidates
from riff.providers import STRING_FORMATS, formatted_value, make_faker, semantic_value
from riff.validation import DRAFT7_DIALECTS, schema_validator

if TYPE_CHECKING:
    from collections.abc import Mapping

    from faker import Faker

    from riff.types import JSONValue, Profile

MAX_DEPTH = 32
MAX_SIZE = 1024
MAX_UNCONSTRAINED_DEPTH = 5
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
        validator = (
            schema_validator(raw, dialect=schema.dialect)
            if isinstance(schema, SchemaNode)
            else schema_validator(raw)
        )
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
            return self._unconstrained_value(depth=depth)
        return self._schema_candidate(schema, depth=depth)

    def _unconstrained_value(self, *, depth: int) -> JSONValue:
        choices: list[JSONValue] = [None, False, True, 0, -1, 42, "", "unexpected", "🦊", [], {}]
        if depth < MAX_UNCONSTRAINED_DEPTH and self.random.randrange(3) == 0:
            if self.random.randrange(2) == 0:
                return [
                    self._candidate(schema=True, depth=depth + 1)
                    for _ in range(self.random.randrange(3))
                ]
            return {
                f"nested_{index}": self._candidate(schema=True, depth=depth + 1)
                for index in range(self.random.randrange(3))
            }
        return self.random.choice(choices)

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
        if schema.condition is not None:
            return self._conditional_value(schema, depth=depth)
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

    def _conditional_value(self, schema: SchemaNode, *, depth: int) -> JSONValue:
        base = schema.model_copy(deep=True)
        base.condition = None
        base.then_schema = None
        base.else_schema = None
        condition = schema.condition
        if condition is None:
            return self._candidate(base, depth=depth + 1)
        if self.random.randrange(2) == 0:
            hint = intersect_nodes(base, condition)
            if schema.then_schema is not None:
                hint = intersect_nodes(hint, schema.then_schema)
        else:
            hint = intersect_nodes(base, negate_node(condition, dialect=schema.dialect))
            if schema.else_schema is not None:
                hint = intersect_nodes(hint, schema.else_schema)
        return self._candidate(hint, depth=depth + 1)

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
        validator = schema_validator(predicate)
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
        if schema.format is not None and schema.format in STRING_FORMATS:
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
        minimum = schema.min_items
        if schema.contains is not None:
            minimum = max(
                minimum,
                schema.min_contains if schema.dialect.rstrip("#") not in DRAFT7_DIALECTS else 1,
            )
        maximum = schema.max_items
        tuple_items = schema.items if isinstance(schema.items, list) else schema.prefix_items
        closed_tuple = (isinstance(schema.items, list) and schema.additional_items is False) or (
            not isinstance(schema.items, list) and schema.items is False
        )
        closed_unevaluated = (
            schema.unevaluated_items is False
            and not schema.items_applied
            and not schema.additional_items_applied
            and schema.contains is None
        )
        if closed_tuple or closed_unevaluated:
            maximum = min(maximum, len(tuple_items)) if maximum is not None else len(tuple_items)
        size = self._size(minimum=minimum, maximum=maximum, default=5)
        values: list[JSONValue] = []
        for index in range(size):
            item_schema = schema.item_schema(index)
            for _ in range(self.max_attempts):
                value = self._validated_value(item_schema, depth=depth + 1)
                if not schema.unique_items or not any(
                    schema_validator({"const": previous}).is_valid(value) for previous in values
                ):
                    values.append(value)
                    break
            else:
                message = "Unable to generate enough distinct array items."
                raise GenerationError(message)
        return self._satisfy_contains(schema, values, depth=depth)

    def _satisfy_contains(
        self, schema: SchemaNode, values: list[JSONValue], *, depth: int
    ) -> list[JSONValue]:
        if schema.contains is None:
            return values
        raw = schema.contains if isinstance(schema.contains, bool) else schema.contains.contract
        checker = schema_validator(raw, dialect=schema.dialect)
        minimum = schema.min_contains if schema.dialect.rstrip("#") not in DRAFT7_DIALECTS else 1
        maximum = schema.max_contains if schema.dialect.rstrip("#") not in DRAFT7_DIALECTS else None
        for index in range(len(values)):
            matches = sum(checker.is_valid(value) for value in values)
            too_few = matches < minimum and not checker.is_valid(values[index])
            too_many = maximum is not None and matches > maximum and checker.is_valid(values[index])
            if not too_few and not too_many:
                continue
            desired = (
                schema.contains if too_few else negate_node(schema.contains, dialect=schema.dialect)
            )
            hint = intersect_nodes(schema.item_schema(index), desired)
            try:
                values[index] = self._validated_value(hint, depth=depth + 1)
            except GenerationError:
                if self.remaining_candidates <= 0:
                    raise
        return values

    def _object(self, schema: SchemaNode, *, depth: int) -> dict[str, JSONValue]:
        selected = list(dict.fromkeys(schema.required))
        optional = [
            name
            for name, child in schema.properties.items()
            if name not in selected and child is not False
        ]
        self.random.shuffle(optional)
        extra_allowed = schema.additional is not False and (
            schema.unevaluated_properties is not False or schema.additional_applied
        )
        extra_capacity = 2 if extra_allowed or schema.patterns else 0
        minimum = max(schema.min_properties, len(selected))
        default_maximum = max(len(schema.properties) + extra_capacity, minimum)
        wanted = self._size(minimum=minimum, maximum=schema.max_properties, default=default_maximum)
        selected.extend(optional[: max(0, wanted - len(selected))])
        selected = self._dependent_keys(schema, selected, depth=depth)
        dependency_hint = self._dependency_hint(schema, selected)
        if isinstance(dependency_hint, SchemaNode):
            return self._object(dependency_hint, depth=depth + 1)
        if dependency_hint is False:
            message = "Triggered dependency has no object values."
            raise GenerationError(message)
        values = {
            name: self._validated_value(self._property_schema(schema, name), depth=depth + 1)
            for name in selected
        }
        return self._additional_values(
            schema, values, wanted=wanted, extra_allowed=extra_allowed, depth=depth
        )

    def _dependent_keys(self, schema: SchemaNode, selected: list[str], *, depth: int) -> list[str]:
        dependencies = dict(schema.dependent_required)
        for name, dependency in schema.dependencies.items():
            if isinstance(dependency, list):
                dependencies[name] = [*dependencies.get(name, []), *dependency]
        required = list(selected)
        for _ in range(len(dependencies) + 1):
            self._spend_candidate(depth=depth)
            additions = [
                child
                for name in required
                for child in dependencies.get(name, [])
                if child not in required
            ]
            if not additions:
                break
            required.extend(dict.fromkeys(additions))
        return required

    def _dependency_hint(self, schema: SchemaNode, selected: list[str]) -> Node | None:
        triggered = {
            name: child for name, child in schema.dependent_schemas.items() if name in selected
        }
        triggered.update(
            {
                name: child
                for name, child in schema.dependencies.items()
                if name in selected and not isinstance(child, list)
            }
        )
        if not triggered:
            return None
        hint: Node = schema.model_copy(deep=True)
        hint.dependent_schemas = {
            name: child for name, child in schema.dependent_schemas.items() if name not in triggered
        }
        hint.dependencies = {
            name: child for name, child in schema.dependencies.items() if name not in triggered
        }
        for child in triggered.values():
            hint = intersect_nodes(hint, child)
        return hint

    def _property_schema(self, schema: SchemaNode, name: str) -> Node:
        matches = [child for pattern, child in schema.patterns.items() if re.search(pattern, name)]
        if name not in schema.properties and not matches:
            if (
                "additionalProperties" not in schema.contract
                and schema.unevaluated_properties is not None
            ):
                return schema.unevaluated_properties
            return (
                schema.additional
                if schema.unevaluated_properties is not False or schema.additional_applied
                else False
            )
        hint = schema.properties.get(name, True)
        for child in matches:
            hint = intersect_nodes(hint, child)
        return hint

    def _additional_values(
        self,
        schema: SchemaNode,
        values: dict[str, JSONValue],
        *,
        wanted: int,
        extra_allowed: bool,
        depth: int,
    ) -> dict[str, JSONValue]:
        if not extra_allowed and not schema.patterns:
            return values
        for index in range(self.max_attempts * 100):
            if len(values) >= wanted:
                break
            self._spend_candidate(depth=depth)
            name = self._extra_name(schema, index, depth=depth)
            if name in values or name in schema.properties:
                continue
            hint = self._property_schema(schema, name)
            if hint is not False:
                values[name] = self._validated_value(hint, depth=depth + 1)
        return values

    def _extra_name(self, schema: SchemaNode, index: int, *, depth: int) -> str:
        patterns = list(schema.patterns)
        if patterns and (schema.additional is False or index % 2 == 0):
            return self.regex.xeger(patterns[index % len(patterns)])
        if schema.property_names is not None:
            hint = intersect_nodes(
                SchemaNode.model_validate({"type": "string"}), schema.property_names
            )
            name = self._validated_value(hint, depth=depth + 1)
            if isinstance(name, str):
                return name
            message = "propertyNames did not produce a string key."
            raise GenerationError(message)
        prefixes = ("extra", "unknown", "garbage")
        return f"{prefixes[index % len(prefixes)]}_{index}"
