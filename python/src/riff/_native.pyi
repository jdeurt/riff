"""Typed fixture generation, metadata, and result contracts."""

from collections.abc import Callable, Mapping, Sequence
from pathlib import Path
from typing import Generic, Literal, TypeAlias, TypeVar, final, overload

from pydantic import BaseModel

from riff.models import ModelFactory
from riff.types import Expectation, JSONValue, Profile, Schema

_ModelT = TypeVar("_ModelT", bound=BaseModel)
_ValueT_co = TypeVar("_ValueT_co", covariant=True)

class RiffError(Exception):
    """A contract, generation, or provider failure."""

class SchemaError(RiffError):
    """Invalid or unsupported contract."""

class GenerationError(RiffError):
    """Bounded generation could not produce a complete batch."""

class ProviderError(RiffError):
    """A provider raised or returned non-JSON data."""

@final
class ProviderContext:
    """Isolated invocation inputs; use seed instead of global randomness."""

    def __init__(
        self,
        seed: int,
        locale: str,
        schema: Mapping[str, JSONValue],
        options: Mapping[str, JSONValue],
    ) -> None: ...
    @property
    def seed(self) -> int: ...
    @property
    def locale(self) -> str: ...
    @property
    def schema(self) -> Mapping[str, JSONValue]: ...
    @property
    def options(self) -> Mapping[str, JSONValue]: ...

@final
class Violation:
    """One validator diagnostic at an RFC 6901 payload pointer."""

    def __init__(self, path: str, message: str) -> None: ...
    @property
    def path(self) -> str: ...
    @property
    def message(self) -> str: ...

@final
class SuccessCase(Generic[_ValueT_co]):
    """A validated successful value with its original JSON payload."""

    def __init__(self, value: _ValueT_co, payload: JSONValue) -> None: ...
    @property
    def value(self) -> _ValueT_co: ...
    @property
    def payload(self) -> JSONValue: ...
    @property
    def expectation(self) -> Literal["pass"]: ...
    @property
    def schema_valid(self) -> Literal[True]: ...
    @property
    def violations(self) -> tuple[Violation, ...]: ...
    @property
    def target(self) -> None: ...
    @property
    def reason(self) -> None: ...

@final
class FailureCase:
    """A deliberate mutation with its observed validity and diagnostics."""

    def __init__(
        self,
        value: JSONValue,
        schema_valid: bool,
        violations: Sequence[Violation],
        target: str,
        reason: str | None = None,
    ) -> None: ...
    @property
    def value(self) -> JSONValue: ...
    @property
    def payload(self) -> JSONValue: ...
    @property
    def expectation(self) -> Literal["fail"]: ...
    @property
    def schema_valid(self) -> bool: ...
    @property
    def violations(self) -> tuple[Violation, ...]: ...
    @property
    def target(self) -> str: ...
    @property
    def reason(self) -> str | None: ...

FixtureCase: TypeAlias = SuccessCase[JSONValue] | FailureCase

@final
class MinimizationResult:
    """A retained reproducer, oracle-call count, and exhaustion state."""

    def __init__(self, value: JSONValue, evaluations: int, budget_exhausted: bool) -> None: ...
    @property
    def value(self) -> JSONValue: ...
    @property
    def evaluations(self) -> int: ...
    @property
    def budget_exhausted(self) -> bool: ...

@final
class SchemaStrategy:
    """Generate from schema constraints; violate is reserved for failures."""

    def __init__(
        self, *, kind: Literal["schema"], constraints: Literal["respect", "violate"] = "respect"
    ) -> None: ...
    @property
    def kind(self) -> Literal["schema"]: ...
    @property
    def constraints(self) -> Literal["respect", "violate"]: ...

@final
class ValuesStrategy:
    """Choose from a nonempty explicit corpus."""

    def __init__(self, *, kind: Literal["values"], values: list[JSONValue]) -> None: ...
    @property
    def kind(self) -> Literal["values"]: ...
    @property
    def values(self) -> list[JSONValue]: ...

@final
class ProviderStrategy:
    """Invoke a registered semantic provider with isolated options."""

    def __init__(
        self,
        *,
        kind: Literal["provider"],
        name: str,
        locale: str = "en-US",
        options: Mapping[str, JSONValue] | None = None,
    ) -> None: ...
    @property
    def kind(self) -> Literal["provider"]: ...
    @property
    def name(self) -> str: ...
    @property
    def locale(self) -> str: ...
    @property
    def options(self) -> dict[str, JSONValue]: ...

@final
class ChoiceStrategy:
    """Choose among nonempty child strategies."""

    def __init__(self, *, kind: Literal["one-of"], strategies: list[Strategy]) -> None: ...
    @property
    def kind(self) -> Literal["one-of"]: ...
    @property
    def strategies(self) -> list[Strategy]: ...

@final
class FilterStrategy:
    """Reject child values that fail an independent schema predicate."""

    def __init__(self, *, kind: Literal["filter"], strategy: Strategy, schema: Schema) -> None: ...
    @classmethod
    def where(cls, strategy: Strategy, *, schema: Schema) -> FilterStrategy:
        """Build a filter from a strategy and a standalone predicate schema."""
    @property
    def kind(self) -> Literal["filter"]: ...
    @property
    def strategy(self) -> Strategy: ...
    @property
    def predicate(self) -> Schema: ...

Strategy: TypeAlias = (
    SchemaStrategy | ValuesStrategy | ProviderStrategy | ChoiceStrategy | FilterStrategy
)

@final
class Failure:
    """Declare negative examples, a provider, or a strategy and an optional reason."""

    def __init__(
        self,
        *,
        provider: str | None = None,
        locale: str | None = None,
        options: Mapping[str, JSONValue] | None = None,
        examples: Sequence[JSONValue] | None = None,
        strategy: Strategy | None = None,
        reason: str | None = None,
    ) -> None: ...
    @property
    def provider(self) -> str | None: ...
    @property
    def locale(self) -> str: ...
    @property
    def options(self) -> dict[str, JSONValue]: ...
    @property
    def examples(self) -> list[JSONValue] | None: ...
    @property
    def strategy(self) -> Strategy | None: ...
    @property
    def reason(self) -> str | None: ...

@overload
def fixture(*, strategy: Strategy, fail: Failure | None = None) -> dict[str, JSONValue]: ...
@overload
def fixture(
    *,
    provider: str | None = None,
    locale: str | None = None,
    options: Mapping[str, JSONValue] | None = None,
    examples: Sequence[JSONValue] | None = None,
    fail: Failure | None = None,
) -> dict[str, JSONValue]:
    """Build validated json_schema_extra metadata for a model field."""

@final
class FixtureFactory:
    """Compile a contract once and generate independent, seeded batches."""

    def __init__(
        self,
        schema: Schema,
        *,
        providers: Mapping[str, Callable[[ProviderContext], JSONValue]] | None = None,
        fixture_file: str | Path | None = None,
    ) -> None: ...
    @classmethod
    def from_schema(
        cls,
        schema: Schema,
        *,
        providers: Mapping[str, Callable[[ProviderContext], JSONValue]] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Compile a standalone Draft 4, Draft 7, 2019-09, or 2020-12 schema."""
    @classmethod
    def from_openapi(
        cls,
        document: dict[str, JSONValue],
        *,
        path: str,
        method: str = "get",
        status: str = "200",
        media_type: str = "application/json",
        body: Literal["request", "response"] = "response",
        providers: Mapping[str, Callable[[ProviderContext], JSONValue]] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Select an exact OpenAPI request or response body."""
    @classmethod
    def from_file(
        cls,
        source: str | Path,
        *,
        path: str | None = None,
        method: str = "get",
        status: str = "200",
        media_type: str = "application/json",
        body: Literal["request", "response"] = "response",
        providers: Mapping[str, Callable[[ProviderContext], JSONValue]] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Read local JSON/YAML and optionally select an OpenAPI body."""
    @classmethod
    def for_model(
        cls,
        model: type[_ModelT],
        *,
        providers: Mapping[str, Callable[[ProviderContext], JSONValue]] | None = None,
    ) -> ModelFactory[_ModelT]:
        """Infer validated outputs from the caller's Pydantic model class."""
    def as_model(self, model: type[_ModelT]) -> ModelFactory[_ModelT]:
        """Bind a model view that retains original payloads and contract validation."""
    def generate(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[JSONValue]:
        """Return a complete valid batch; raise on bounded search exhaustion."""
    def success_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[SuccessCase[JSONValue]]:
        """Return successful cases with literal pass and valid types."""
    def failure_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FailureCase]:
        """Mutate declared failures and retain raw payloads and diagnostics."""
    def generate_cases(
        self,
        *,
        count: int = 1,
        expectation: Expectation = "pass",
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FixtureCase]:
        """Keep declared outcomes separate from observed schema validity."""
    def minimize(
        self,
        value: JSONValue,
        *,
        is_failure: Callable[[JSONValue], bool],
        max_evaluations: int = 100,
        preserve_schema: bool = True,
    ) -> MinimizationResult:
        """Reduce a reproducer with the caller's failure oracle."""
    def is_valid(self, value: JSONValue) -> bool:
        """Check JSON against the original contract without generation."""

def strip_fixture_metadata(document: dict[str, JSONValue]) -> dict[str, JSONValue]:
    """Remove fixture annotations from a copy while preserving example data."""

def encode_json(value: JSONValue) -> str:
    """Serialize a JSON value without accepting nonfinite numbers."""

def cli(arguments: list[str]) -> tuple[str, str, int]:
    """Return stdout, stderr, and exit code for the complete CLI workflow."""

def model_is_valid(factory: FixtureFactory, model: BaseModel) -> bool:
    """Check model serialization against the factory's original contract."""
