"""Language-neutral JSON values and generation profiles."""

from typing import Literal, Protocol, TypeAlias

from typing_extensions import TypeAliasType

from riff._native import FailureCase, ProviderContext, SuccessCase

JSONValue = TypeAliasType(
    "JSONValue", "bool | int | float | str | list[JSONValue] | dict[str, JSONValue] | None"
)
Schema: TypeAlias = dict[str, JSONValue] | bool
Profile: TypeAlias = Literal["mixed", "realistic", "boundary"]
Expectation: TypeAlias = Literal["pass", "fail"]

FixtureCase: TypeAlias = SuccessCase[JSONValue] | FailureCase


class Provider(Protocol):
    """Produce JSON; identical contexts should produce identical values."""

    def __call__(self, context: ProviderContext, /) -> JSONValue:
        """Return one value without changing external state."""
        ...
