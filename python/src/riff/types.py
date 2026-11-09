"""Language-neutral JSON values and generation profiles."""

from typing import Literal, TypeAlias

from typing_extensions import TypeAliasType

JSONValue = TypeAliasType(
    "JSONValue", "bool | int | float | str | list[JSONValue] | dict[str, JSONValue] | None"
)
Schema: TypeAlias = dict[str, JSONValue] | bool
Profile: TypeAlias = Literal["mixed", "realistic", "boundary"]
Expectation: TypeAlias = Literal["pass", "fail"]
