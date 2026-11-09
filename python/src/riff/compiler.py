"""Compile untrusted contracts into typed schema and strategy representations."""

from __future__ import annotations

from typing import TYPE_CHECKING, Annotated, Literal

from faker.config import AVAILABLE_LOCALES
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    ModelWrapValidatorHandler,
    PrivateAttr,
    TypeAdapter,
    ValidationError,
    field_serializer,
    field_validator,
    model_validator,
)
from typing_extensions import Self, TypeAliasType

from riff.errors import SchemaError
from riff.schema import normalize_schema
from riff.types import JSONValue, Schema
from riff.validation import DEFAULT_DIALECT, DRAFT7_DIALECTS, schema_validator

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping

    from riff.contracts import Provider

JSON_ADAPTER = TypeAdapter[JSONValue](
    JSONValue, config=ConfigDict(strict=True, allow_inf_nan=False)
)
BUILTINS = frozenset(
    {"email", "full-name", "company-name", "url", "phone-number", "city", "country-code", "uuid"}
)


class StrategyModel(BaseModel):
    """Reject misspelled strategy and metadata fields at the input boundary."""

    model_config = ConfigDict(extra="forbid", strict=True)


class SchemaStrategy(StrategyModel):
    """Generate from the enclosing schema without its fixture annotation."""

    kind: Literal["schema"]
    constraints: Literal["respect", "violate"] = "respect"


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
        document = (
            value.contract if isinstance(value, SchemaNode) else JSON_ADAPTER.validate_python(value)
        )
        return normalize_schema(document, document, openapi30=False)

    @classmethod
    def where(cls, strategy: Strategy, *, schema: Schema) -> Self:
        """Construct a typed filter directly from an ordinary predicate schema."""
        return cls.model_validate(
            {
                "kind": "filter",
                "strategy": strategy.model_dump(mode="json", by_alias=True, exclude_unset=True),
                "schema": schema,
            }
        )

    @field_serializer("predicate")
    def serialize_predicate(self, predicate: Node) -> Schema:
        """Preserve the predicate's declared dialect and complete schema on export."""
        return predicate if isinstance(predicate, bool) else predicate.contract


Strategy = TypeAliasType(
    "Strategy",
    Annotated[
        SchemaStrategy | ValuesStrategy | ProviderStrategy | ChoiceStrategy | FilterStrategy,
        Field(discriminator="kind"),
    ],
)


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
        null_fields = {
            "provider": self.provider is None,
            "examples": self.examples is None,
            "strategy": self.strategy is None,
        }
        for name, is_null in null_fields.items():
            if name in self.model_fields_set and is_null:
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
    items: Node | list[Node] = True
    prefix_items: list[Node] = Field(default_factory=list, alias="prefixItems")
    additional_items: Node = Field(default=True, alias="additionalItems")
    min_contains: int = Field(default=1, alias="minContains")
    max_contains: int | None = Field(default=None, alias="maxContains")
    property_names: Node | None = Field(default=None, alias="propertyNames")
    dependent_required: dict[str, list[str]] = Field(
        default_factory=dict, alias="dependentRequired"
    )
    dependent_schemas: dict[str, Node] = Field(default_factory=dict, alias="dependentSchemas")
    dependencies: dict[str, Node | list[str]] = Field(default_factory=dict)
    condition: Node | None = Field(default=None, alias="if")
    then_schema: Node | None = Field(default=None, alias="then")
    else_schema: Node | None = Field(default=None, alias="else")
    unevaluated_properties: Node | None = Field(default=None, alias="unevaluatedProperties")
    unevaluated_items: Node | None = Field(default=None, alias="unevaluatedItems")
    content_schema: Node | None = Field(default=None, alias="contentSchema")
    dialect: str = Field(default=DEFAULT_DIALECT, exclude=True)
    items_applied: bool = Field(default=False, exclude=True)
    additional_applied: bool = Field(default=False, exclude=True)
    additional_items_applied: bool = Field(default=False, exclude=True)
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
        parsed.items_applied = "items" in contract and not isinstance(contract["items"], list)
        parsed.additional_applied = "additionalProperties" in contract
        parsed.additional_items_applied = (
            isinstance(parsed.items, list) and "additionalItems" in contract
        )
        return parsed

    @property
    def contract(self) -> dict[str, JSONValue]:
        """Normalized JSON Schema used by the independent validation oracle."""
        return self._contract

    @contract.setter
    def contract(self, value: dict[str, JSONValue]) -> None:
        """Keep changes to the schema representation inside its owning module."""
        self._contract = value

    def item_schema(self, index: int) -> Node:
        """Apply tuple/prefix semantics in the schema's inherited dialect."""
        if isinstance(self.items, list):
            return self._tuple_item_schema(self.items, index)
        modern = (
            self.dialect.rstrip("#") not in DRAFT7_DIALECTS
            and self.dialect != "https://json-schema.org/draft/2019-09/schema"
        )
        if modern and index < len(self.prefix_items):
            return self.prefix_items[index]
        if not self.items_applied and self.unevaluated_items is not None:
            if self.contains is None:
                return self.unevaluated_items
            if self.unevaluated_items is False:
                return self.contains
        return self.items

    def _tuple_item_schema(self, items: list[Node], index: int) -> Node:
        if index < len(items):
            return items[index]
        if (
            not self.additional_items_applied
            and not self.items_applied
            and self.unevaluated_items is not None
        ):
            return self.unevaluated_items
        return self.additional_items

    def child_locations(self) -> Iterator[tuple[tuple[str | int, ...], Node]]:
        """Describe all schema-bearing locations once for validation and sidecars."""
        for keyword, mapping in (
            ("properties", self.properties),
            ("patternProperties", self.patterns),
            ("dependentSchemas", self.dependent_schemas),
        ):
            for name, child in mapping.items():
                yield (keyword, name), child
        for keyword, sequence in (
            ("allOf", self.all_of),
            ("oneOf", self.one_of),
            ("anyOf", self.any_of),
            ("prefixItems", self.prefix_items),
        ):
            for index, child in enumerate(sequence):
                yield (keyword, index), child
        if isinstance(self.items, list):
            for index, child in enumerate(self.items):
                yield ("items", index), child
        else:
            yield ("items",), self.items
        yield from self._single_child_locations()

    def _single_child_locations(self) -> Iterator[tuple[tuple[str | int, ...], Node]]:
        for name, dependency in self.dependencies.items():
            if not isinstance(dependency, list):
                yield ("dependencies", name), dependency
        for keyword, child in (
            ("additionalProperties", self.additional),
            ("additionalItems", self.additional_items),
            ("propertyNames", self.property_names),
            ("if", self.condition),
            ("then", self.then_schema),
            ("else", self.else_schema),
            ("unevaluatedProperties", self.unevaluated_properties),
            ("unevaluatedItems", self.unevaluated_items),
            ("not", self.negated),
            ("contains", self.contains),
            ("contentSchema", self.content_schema),
        ):
            if child is not None:
                yield (keyword,), child

    def children(self) -> Iterator[Node]:
        """Visit schema children while leaving annotation data untouched."""
        yield from (child for _, child in self.child_locations())


Node = TypeAliasType("Node", SchemaNode | bool)
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


def _validate_node(
    node: Node, providers: Mapping[str, Provider], *, dialect: str = DEFAULT_DIALECT
) -> None:
    if isinstance(node, bool):
        return
    declared = node.contract.get("$schema")
    node.dialect = (declared if isinstance(declared, str) else dialect).rstrip("#")
    _deactivate_inert_keywords(node)
    _validate_source(node.fixture, node, providers, allow_violations=False)
    if node.fixture.fail is not None:
        _validate_source(node.fixture.fail, node, providers, allow_violations=True)
    validator = schema_validator(node.contract, dialect=node.dialect)
    for example in node.fixture.examples or []:
        if not validator.is_valid(example):
            message = "x-fixture.examples contains an invalid value"
            raise SchemaError(message)
    for child in node.children():
        _validate_node(child, providers, dialect=node.dialect)


def _validate_source(
    source: Source, node: SchemaNode, providers: Mapping[str, Provider], *, allow_violations: bool
) -> None:
    if source.provider is not None:
        _validate_provider(source.provider, source.locale, node, providers)
    if source.strategy is not None:
        _validate_strategy(source.strategy, node, providers, allow_violations=allow_violations)


def _validate_provider(
    name: str, locale: str, node: SchemaNode, providers: Mapping[str, Provider]
) -> None:
    if name in providers:
        return
    if name not in BUILTINS:
        message = f"Unknown semantic provider: {name}"
        raise SchemaError(message)
    if not _permits_string_provider(node):
        message = "Default semantic providers require a string schema"
        raise SchemaError(message)

    if locale.replace("-", "_") not in AVAILABLE_LOCALES:
        message = f"Unsupported locale: {locale}"
        raise SchemaError(message)


def _validate_strategy(
    strategy: Strategy,
    node: SchemaNode,
    providers: Mapping[str, Provider],
    *,
    allow_violations: bool,
) -> None:
    if isinstance(strategy, SchemaStrategy):
        if strategy.constraints == "violate" and not allow_violations:
            message = "constraints: violate is allowed only inside an explicit failure source"
            raise SchemaError(message)
    elif isinstance(strategy, ProviderStrategy):
        _validate_provider(strategy.name, strategy.locale, node, providers)
    elif isinstance(strategy, ChoiceStrategy):
        for child in strategy.strategies:
            _validate_strategy(child, node, providers, allow_violations=allow_violations)
    elif isinstance(strategy, FilterStrategy):
        _validate_node(strategy.predicate, providers)
        _validate_strategy(strategy.strategy, node, providers, allow_violations=allow_violations)


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
    _merge_structural_constraints(result, right)
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


def negate_node(node: Node, *, dialect: str) -> Node:
    """Compose a typed negation used for conditional and contains exploration."""
    if isinstance(node, bool):
        return not node
    result = SchemaNode.model_validate({"not": node.contract})
    result.dialect = dialect
    return result


def _merge_structural_constraints(result: SchemaNode, right: SchemaNode) -> None:
    result.items_applied = result.items_applied or right.items_applied
    result.additional_applied = result.additional_applied or right.additional_applied
    result.additional_items_applied = (
        result.additional_items_applied or right.additional_items_applied
    )
    _merge_array_constraints(result, right)
    result.patterns.update(right.patterns)
    result.dependent_schemas.update(right.dependent_schemas)
    result.dependencies.update(right.dependencies)
    for name, required in right.dependent_required.items():
        result.dependent_required[name] = list(
            dict.fromkeys([*result.dependent_required.get(name, []), *required])
        )
    if right.unevaluated_properties is not None:
        result.unevaluated_properties = right.unevaluated_properties
    if right.unevaluated_items is not None:
        result.unevaluated_items = right.unevaluated_items
    if right.property_names is not None:
        result.property_names = intersect_nodes(
            result.property_names if result.property_names is not None else True,
            right.property_names,
        )


def _deactivate_inert_keywords(node: SchemaNode) -> None:
    """Keep generation hints aligned with keywords active in the selected dialect."""
    if node.dialect in DRAFT7_DIALECTS:
        node.dependent_required = {}
        node.dependent_schemas = {}
        node.unevaluated_properties = None
        node.unevaluated_items = None
        node.min_contains = 1
        node.max_contains = None
    else:
        node.dependencies = {}
    if (
        node.dialect in DRAFT7_DIALECTS
        or node.dialect == "https://json-schema.org/draft/2019-09/schema"
    ):
        node.prefix_items = []


def accepts_value(node: Node, value: JSONValue) -> bool:
    """Evaluate a typed schema against a value using its inherited dialect."""
    if isinstance(node, bool):
        return node
    return schema_validator(node.contract, dialect=node.dialect).is_valid(value)


def applicable_branches(node: SchemaNode, value: JSONValue) -> Iterator[Node]:
    """Visit same-instance schema scopes active for a known valid fixture."""
    yield from node.all_of
    for branch in [*node.one_of, *node.any_of]:
        if accepts_value(branch, value):
            yield branch
    if node.condition is not None:
        if accepts_value(node.condition, value):
            yield node.condition
            if node.then_schema is not None:
                yield node.then_schema
        elif node.else_schema is not None:
            yield node.else_schema
    if isinstance(value, dict):
        yield from _triggered_dependencies(node, value)


def _triggered_dependencies(node: SchemaNode, value: dict[str, JSONValue]) -> Iterator[Node]:
    for name, branch in node.dependent_schemas.items():
        if name in value:
            yield branch
    for name, branch in node.dependencies.items():
        if name in value and not isinstance(branch, list):
            yield branch


def _permits_string_provider(node: SchemaNode) -> bool:
    if isinstance(node.types, str):
        return node.types == "string"
    if isinstance(node.types, list):
        return "string" in node.types
    alternatives = node.any_of or node.one_of
    if alternatives:
        return any(
            branch is True or (isinstance(branch, SchemaNode) and _permits_string_provider(branch))
            for branch in alternatives
        )
    return True


def _merge_array_constraints(result: SchemaNode, right: SchemaNode) -> None:
    if not isinstance(result.items, list) and not isinstance(right.items, list):
        result.items = intersect_nodes(result.items, right.items)
    elif isinstance(right.items, list):
        result.items = right.items
    if right.prefix_items:
        result.prefix_items = right.prefix_items
    result.additional_items = intersect_nodes(result.additional_items, right.additional_items)
    if result.contains is None and right.contains is not None:
        result.contains = right.contains
        result.min_contains = right.min_contains
        result.max_contains = right.max_contains
