"""Compile untrusted contracts into typed schema and strategy representations."""

from __future__ import annotations

from typing import TYPE_CHECKING, Annotated, Literal, Self

from faker.config import AVAILABLE_LOCALES
from jsonschema import Draft7Validator, FormatChecker
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    ModelWrapValidatorHandler,
    PrivateAttr,
    TypeAdapter,
    ValidationError,
    field_validator,
    model_validator,
)

from riff.errors import SchemaError
from riff.schema import normalize_schema
from riff.types import JSONValue

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping

    from riff.contracts import Provider

JSON_ADAPTER = TypeAdapter[JSONValue](
    JSONValue, config=ConfigDict(strict=True, allow_inf_nan=False)
)
BUILTINS = frozenset(
    {"email", "full-name", "company-name", "url", "phone-number", "city", "country-code", "uuid"}
)
FORMATS = frozenset(
    {
        "email",
        "uuid",
        "uri",
        "hostname",
        "ipv4",
        "ipv6",
        "date",
        "date-time",
        "int32",
        "int64",
        "float",
        "double",
    }
)


class StrategyModel(BaseModel):
    """Reject misspelled strategy and metadata fields at the input boundary."""

    model_config = ConfigDict(extra="forbid", strict=True)


class SchemaStrategy(StrategyModel):
    """Generate from the enclosing schema without its fixture annotation."""

    kind: Literal["schema"]


class ValuesStrategy(StrategyModel):
    """Choose a value from a nonempty explicit corpus."""

    kind: Literal["values"]
    values: list[JSONValue] = Field(min_length=1)


class ProviderStrategy(StrategyModel):
    """Invoke a registered semantic provider."""

    kind: Literal["provider"]
    name: str
    locale: str = "en-US"
    options: dict[str, JSONValue] = Field(default_factory=dict)


class ChoiceStrategy(StrategyModel):
    """Choose among child strategies."""

    kind: Literal["one-of"]
    strategies: list[Strategy] = Field(min_length=1)


class FilterStrategy(StrategyModel):
    """Reject child values that fail a portable schema predicate."""

    kind: Literal["filter"]
    strategy: Strategy
    predicate: Node = Field(alias="schema")

    @field_validator("predicate", mode="before")
    @classmethod
    def compile_predicate(cls, value: object) -> JSONValue:
        """Resolve and validate predicate schemas at the metadata boundary."""
        document = JSON_ADAPTER.validate_python(value)
        return normalize_schema(document, document, openapi30=False)


type Strategy = Annotated[
    SchemaStrategy | ValuesStrategy | ProviderStrategy | ChoiceStrategy | FilterStrategy,
    Field(discriminator="kind"),
]


class Source(StrategyModel):
    """A provider/example shorthand or one explicit strategy tree."""

    provider: str | None = None
    locale: str = "en-US"
    options: dict[str, JSONValue] = Field(default_factory=dict)
    examples: list[JSONValue] | None = Field(default=None, min_length=1)
    strategy: Strategy | None = None

    @model_validator(mode="after")
    def check_source(self) -> Self:
        """Reject ambiguous sources and options without a provider."""
        if self.strategy is not None and self.model_fields_set.intersection(
            {"provider", "locale", "options", "examples"}
        ):
            message = "strategy cannot be combined with provider, locale, options, or examples"
            raise ValueError(message)
        if self.provider is None and self.model_fields_set.intersection({"locale", "options"}):
            message = "locale and options require a provider"
            raise ValueError(message)
        for name in ("provider", "examples", "strategy"):
            if name in self.model_fields_set and getattr(self, name) is None:
                message = f"{name} must not be null"
                raise ValueError(message)
        return self


class FailureSource(Source):
    """An explicit negative source with a consumer-facing explanation."""

    reason: str | None = None

    @model_validator(mode="after")
    def require_source(self) -> Self:
        """Require data that can actually replace a field."""
        if self.provider is None and self.examples is None and self.strategy is None:
            message = "fail requires provider, examples, or strategy"
            raise ValueError(message)
        return self


class FixtureMetadata(Source):
    """Portable positive and negative value-generation hints."""

    fail: FailureSource | None = None


class SchemaNode(BaseModel):
    """Typed schema keywords; the untouched normalized contract remains the oracle."""

    model_config = ConfigDict(extra="ignore", strict=True)
    _contract: dict[str, JSONValue] = PrivateAttr(default_factory=dict)
    types: str | list[str] | None = Field(default=None, alias="type")
    enum: list[JSONValue] | None = None
    const: JSONValue = None
    properties: dict[str, Node] = Field(default_factory=dict)
    required: list[str] = Field(default_factory=list)
    additional: Node = Field(default=True, alias="additionalProperties")
    patterns: dict[str, Node] = Field(default_factory=dict, alias="patternProperties")
    items: Node = True
    all_of: list[Node] = Field(default_factory=list, alias="allOf")
    any_of: list[Node] = Field(default_factory=list, alias="anyOf")
    one_of: list[Node] = Field(default_factory=list, alias="oneOf")
    negated: Node | None = Field(default=None, alias="not")
    contains: Node | None = None
    minimum: int | float | None = None
    maximum: int | float | None = None
    exclusive_minimum: int | float | None = Field(default=None, alias="exclusiveMinimum")
    exclusive_maximum: int | float | None = Field(default=None, alias="exclusiveMaximum")
    multiple_of: int | float | None = Field(default=None, alias="multipleOf")
    min_length: int = Field(default=0, alias="minLength")
    max_length: int | None = Field(default=None, alias="maxLength")
    pattern: str | None = None
    format: str | None = None
    min_items: int = Field(default=0, alias="minItems")
    max_items: int | None = Field(default=None, alias="maxItems")
    unique_items: bool = Field(default=False, alias="uniqueItems")
    min_properties: int = Field(default=0, alias="minProperties")
    max_properties: int | None = Field(default=None, alias="maxProperties")
    fixture: FixtureMetadata = Field(default_factory=FixtureMetadata, alias="x-fixture")

    @model_validator(mode="wrap")
    @classmethod
    def capture_contract(cls, value: object, handler: ModelWrapValidatorHandler[Self]) -> Self:
        """Parse input once, preserving its exact JSON Schema validation contract."""
        parsed = handler(value)
        contract = JSON_ADAPTER.validate_python(value)
        if not isinstance(contract, dict):
            message = "Expected a schema object"
            raise TypeError(message)
        parsed.contract = contract
        return parsed

    @property
    def contract(self) -> dict[str, JSONValue]:
        """Normalized JSON Schema used by the independent validation oracle."""
        return self._contract

    @contract.setter
    def contract(self, value: dict[str, JSONValue]) -> None:
        """Keep changes to the schema representation inside its owning module."""
        self._contract = value

    def children(self) -> Iterator[Node]:
        """Visit schema children without interpreting metadata as schema data."""
        yield from self.properties.values()
        yield from self.patterns.values()
        yield from self.all_of
        yield from self.any_of
        yield from self.one_of
        yield self.items
        yield self.additional
        if self.negated is not None:
            yield self.negated
        if self.contains is not None:
            yield self.contains


type Node = SchemaNode | bool
NODE_ADAPTER = TypeAdapter[Node](Node)


def compile_schema(schema: JSONValue, providers: Mapping[str, Provider]) -> Node:
    """Validate the boundary representation and all declared provider references."""
    try:
        node = NODE_ADAPTER.validate_python(schema, strict=True)
        _validate_node(node, providers)
    except ValidationError as error:
        message = f"Invalid fixture metadata: {error}"
        raise SchemaError(message) from error
    return node


def _validate_node(node: Node, providers: Mapping[str, Provider]) -> None:
    if isinstance(node, bool):
        return
    if node.format is not None and node.format not in FORMATS:
        message = f"Unsupported string format: {node.format}"
        raise SchemaError(message)
    _validate_source(node.fixture, node, providers)
    if node.fixture.fail is not None:
        _validate_source(node.fixture.fail, node, providers)
    validator = Draft7Validator(node.contract, format_checker=FormatChecker())
    for example in node.fixture.examples or []:
        if not validator.is_valid(example):
            message = "x-fixture.examples contains an invalid value"
            raise SchemaError(message)
    for child in node.children():
        _validate_node(child, providers)


def _validate_source(source: Source, node: SchemaNode, providers: Mapping[str, Provider]) -> None:
    if source.provider is not None:
        _validate_provider(source.provider, source.locale, node, providers)
    if source.strategy is not None:
        _validate_strategy(source.strategy, node, providers)


def _validate_provider(
    name: str, locale: str, node: SchemaNode, providers: Mapping[str, Provider]
) -> None:
    if name in providers:
        return
    if name not in BUILTINS:
        message = f"Unknown semantic provider: {name}"
        raise SchemaError(message)
    if node.types != "string" and not (isinstance(node.types, list) and "string" in node.types):
        message = "Default semantic providers require a string schema"
        raise SchemaError(message)

    if locale.replace("-", "_") not in AVAILABLE_LOCALES:
        message = f"Unsupported locale: {locale}"
        raise SchemaError(message)


def _validate_strategy(
    strategy: Strategy, node: SchemaNode, providers: Mapping[str, Provider]
) -> None:
    if isinstance(strategy, ProviderStrategy):
        _validate_provider(strategy.name, strategy.locale, node, providers)
    elif isinstance(strategy, ChoiceStrategy):
        for child in strategy.strategies:
            _validate_strategy(child, node, providers)
    elif isinstance(strategy, FilterStrategy):
        _validate_node(strategy.predicate, providers)
        _validate_strategy(strategy.strategy, node, providers)


def intersect_nodes(left: Node, right: Node) -> Node:
    """Combine typed generation hints while retaining the full intersection oracle."""
    if left is False or right is False:
        return False
    if left is True:
        return right
    if right is True:
        return left
    result = left.model_copy(deep=True)
    result.contract = {"allOf": [left.contract, right.contract]}
    result.required = list(dict.fromkeys([*left.required, *right.required]))
    for name, child in right.properties.items():
        result.properties[name] = intersect_nodes(result.properties.get(name, True), child)
    result.additional = intersect_nodes(left.additional, right.additional)
    result.min_length = max(left.min_length, right.min_length)
    result.min_items = max(left.min_items, right.min_items)
    result.min_properties = max(left.min_properties, right.min_properties)
    result.unique_items = left.unique_items or right.unique_items
    result.items = intersect_nodes(left.items, right.items)
    _merge_optional_constraints(result, right)
    _merge_sources_and_choices(result, right)
    return result


def _merge_sources_and_choices(result: SchemaNode, right: SchemaNode) -> None:
    if right.fixture.model_fields_set:
        result.fixture = right.fixture
    if right.types is not None:
        result.types = right.types
    if right.enum is not None:
        result.enum = right.enum
    if "const" in right.model_fields_set:
        result.const = right.const
        result.model_fields_set.add("const")
    if right.all_of:
        result.all_of = right.all_of
    if right.any_of:
        result.any_of = right.any_of
    if right.one_of:
        result.one_of = right.one_of


def _merge_optional_constraints(result: SchemaNode, right: SchemaNode) -> None:
    if right.minimum is not None:
        result.minimum = (
            right.minimum if result.minimum is None else max(result.minimum, right.minimum)
        )
    if right.maximum is not None:
        result.maximum = (
            right.maximum if result.maximum is None else min(result.maximum, right.maximum)
        )
    if right.exclusive_minimum is not None:
        result.exclusive_minimum = (
            right.exclusive_minimum
            if result.exclusive_minimum is None
            else max(result.exclusive_minimum, right.exclusive_minimum)
        )
    if right.exclusive_maximum is not None:
        result.exclusive_maximum = (
            right.exclusive_maximum
            if result.exclusive_maximum is None
            else min(result.exclusive_maximum, right.exclusive_maximum)
        )
    _merge_collection_bounds(result, right)
    if right.multiple_of is not None:
        result.multiple_of = right.multiple_of
    if right.pattern is not None:
        result.pattern = right.pattern
    if right.format is not None:
        result.format = right.format


def _merge_collection_bounds(result: SchemaNode, right: SchemaNode) -> None:
    if right.max_length is not None:
        result.max_length = (
            right.max_length
            if result.max_length is None
            else min(result.max_length, right.max_length)
        )
    if right.max_items is not None:
        result.max_items = (
            right.max_items if result.max_items is None else min(result.max_items, right.max_items)
        )
    if right.max_properties is not None:
        result.max_properties = (
            right.max_properties
            if result.max_properties is None
            else min(result.max_properties, right.max_properties)
        )
