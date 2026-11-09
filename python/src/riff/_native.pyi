"""Private declarations; use riff.FixtureFactory publicly."""

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
    """Immutable fixture contract supporting independent generation calls."""

    def __init__(self, source: str, configuration: str) -> None: ...
    def generate(self, options: str, provider: Callable[[str], str]) -> str:
        """Return a complete JSON batch or raise a domain exception."""
    def cases(self, options: str, provider: Callable[[str], str]) -> str:
        """Return labeled case envelopes with original payloads and diagnostics."""
    def minimize(self, value: str, options: str, oracle: Callable[[str], bool]) -> str:
        """Retain reductions that reproduce the caller's failure."""
    def is_valid(self, value: str) -> bool:
        """Check model serialization against the original contract."""

def strip_metadata(document: str) -> str:
    """Remove schema-only fixture annotations from an OpenAPI copy."""

def validate_metadata(metadata: str) -> None:
    """Validate a complete positive/negative fixture annotation."""

def cli(arguments: list[str]) -> tuple[str, str, int]:
    """Return stdout, stderr, and exit code for the complete CLI workflow."""
