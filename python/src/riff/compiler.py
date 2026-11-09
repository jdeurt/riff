"""Typed metadata builders; the Rust core interprets and validates contracts."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    TypeAdapter,
    field_serializer,
    field_validator,
    model_validator,
)
from typing_extensions import Self, TypeAliasType

from riff.types import JSONValue, Schema

JSON_ADAPTER = TypeAdapter[JSONValue](
    JSONValue, config=ConfigDict(strict=True, allow_inf_nan=False)
)
SCHEMA_ADAPTER = TypeAdapter[Schema](Schema, config=ConfigDict(strict=True, allow_inf_nan=False))


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
    predicate: Schema = Field(alias="schema")

    @field_validator("predicate", mode="before")
    @classmethod
    def validate_predicate(cls, value: object) -> JSONValue:
        """Validate predicate JSON shape; Rust resolves and compiles the schema."""
        return SCHEMA_ADAPTER.validate_python(value)

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
    def serialize_predicate(self, predicate: Schema) -> Schema:
        """Preserve the predicate's declared dialect and complete schema on export."""
        return predicate


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
