"""Language-neutral JSON values and generation profiles."""

from typing import Literal

type JSONValue = bool | int | float | str | list[JSONValue] | dict[str, JSONValue] | None
type Schema = dict[str, JSONValue] | bool
type Profile = Literal["mixed", "realistic", "boundary"]
type Expectation = Literal["pass", "fail"]
