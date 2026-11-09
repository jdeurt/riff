"""Generate request and response fixtures through Riff's implementation-neutral interface."""

from riff.annotations import Failure, fixture
from riff.compiler import (
    ChoiceStrategy,
    FilterStrategy,
    ProviderStrategy,
    SchemaStrategy,
    ValuesStrategy,
)
from riff.contracts import (
    FailureCase,
    FixtureCase,
    Provider,
    ProviderContext,
    SuccessCase,
    Violation,
)
from riff.errors import GenerationError, ProviderError, RiffError, SchemaError
from riff.factory import FixtureFactory
from riff.minimize import MinimizationResult
from riff.models import ModelFactory
from riff.publication import strip_fixture_metadata
from riff.types import Expectation, JSONValue, Profile

__all__ = [
    "ChoiceStrategy",
    "Expectation",
    "Failure",
    "FailureCase",
    "FilterStrategy",
    "FixtureCase",
    "FixtureFactory",
    "GenerationError",
    "JSONValue",
    "MinimizationResult",
    "ModelFactory",
    "Profile",
    "Provider",
    "ProviderContext",
    "ProviderError",
    "ProviderStrategy",
    "RiffError",
    "SchemaError",
    "SchemaStrategy",
    "SuccessCase",
    "ValuesStrategy",
    "Violation",
    "fixture",
    "strip_fixture_metadata",
]
