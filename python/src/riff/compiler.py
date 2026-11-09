"""Typed declarations for portable fixture metadata."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    TypeAdapter,
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
    values: list[JSONValue]


class ProviderStrategy(StrategyModel):
    """Invoke a registered semantic provider."""

    kind: Literal["provider"]
    name: str
    locale: str = "en-US"
    options: dict[str, JSONValue] = Field(default_factory=dict)


class ChoiceStrategy(StrategyModel):
    """Choose among child strategies."""

    kind: Literal["one-of"]
    strategies: list[Strategy]


class FilterStrategy(StrategyModel):
    """Reject child values that fail a portable schema predicate."""

    kind: Literal["filter"]
    strategy: Strategy
    predicate: Schema = Field(alias="schema")

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
    examples: list[JSONValue] | None = None
    strategy: Strategy | None = None


class FailureSource(Source):
    """An explicit negative source with a consumer-facing explanation."""

    reason: str | None = None
