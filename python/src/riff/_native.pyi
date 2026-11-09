"""Private typed ABI for the PyO3 extension; use riff.FixtureFactory publicly."""

from collections.abc import Callable

class RiffError(Exception):
    """A contract, generation, or provider failure."""

class SchemaError(RiffError):
    """Invalid or unsupported contract."""

class GenerationError(RiffError):
    """Bounded generation could not produce a complete batch."""

class ProviderError(RiffError):
    """A provider raised or returned non-JSON data."""

class NativeFactory:
    """Immutable compiled Rust contract; each call owns its random state."""

    def __init__(
        self,
        document: str,
        names: list[str],
        locales: list[str],
        operation: str | None = None,
        fixtures: str | None = None,
    ) -> None: ...
    def generate(self, options: str, provider: Callable[[str], str]) -> str:
        """Return a complete JSON batch or raise a domain exception."""
    def failure_cases(self, options: str, provider: Callable[[str], str]) -> str:
        """Return raw case envelopes with independent validity diagnostics."""
    def minimize(
        self, value: str, maximum: int, preserve: bool, oracle: Callable[[str], bool]
    ) -> str:
        """Retain reductions that reproduce the caller's failure."""
    def is_valid(self, value: str) -> bool:
        """Check model serialization against the original contract."""
    def schema(self) -> str:
        """Return the normalized schema as JSON."""

def strip_metadata(document: str) -> str:
    """Remove schema-only fixture annotations from an OpenAPI copy."""
