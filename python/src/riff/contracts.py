"""Portable provider and fixture-case contracts."""

from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Generic, Literal, Protocol, TypeAlias, TypeVar

from riff.types import JSONValue

ValueT_co = TypeVar("ValueT_co", covariant=True)


@dataclass(frozen=True)
class ProviderContext:
    """Per-invocation inputs; providers must use seed rather than global randomness."""

    seed: int
    locale: str
    schema: Mapping[str, JSONValue]
    options: Mapping[str, JSONValue]


class Provider(Protocol):
    """Produce one JSON value; identical contexts should produce identical values."""

    def __call__(self, context: ProviderContext, /) -> JSONValue:
        """Return a value, leaving external state unchanged."""
        ...


@dataclass(frozen=True)
class Violation:
    """One schema violation at an RFC 6901 JSON Pointer."""

    path: str
    message: str


@dataclass(frozen=True)
class SuccessCase(Generic[ValueT_co]):
    """A validated successful payload; its outcome and validity have literal types."""

    value: ValueT_co
    payload: JSONValue
    expectation: Literal["pass"] = field(default="pass", init=False)
    schema_valid: Literal[True] = field(default=True, init=False)
    violations: tuple[Violation, ...] = field(default=(), init=False)
    target: None = field(default=None, init=False)
    reason: None = field(default=None, init=False)


@dataclass(frozen=True)
class FailureCase:
    """An intentionally changed raw payload, which may violate the schema."""

    value: JSONValue
    schema_valid: bool
    violations: tuple[Violation, ...]
    target: str
    reason: str | None = None
    expectation: Literal["fail"] = field(default="fail", init=False)

    @property
    def payload(self) -> JSONValue:
        """The exact JSON body to supply to the system under test."""
        return self.value


FixtureCase: TypeAlias = SuccessCase[JSONValue] | FailureCase


def case_json(case: FixtureCase) -> dict[str, JSONValue]:
    """Serialize a raw case envelope without losing its outcome or diagnostics."""
    return {
        "value": case.value,
        "expectation": case.expectation,
        "schema_valid": case.schema_valid,
        "violations": [
            {"path": violation.path, "message": violation.message} for violation in case.violations
        ],
        "target": case.target,
        "reason": case.reason,
    }
