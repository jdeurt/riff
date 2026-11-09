"""Public failures reported by Riff."""


class RiffError(Exception):
    """Base class for errors callers can handle without backend knowledge."""


class SchemaError(RiffError):
    """The supplied contract is invalid or outside the supported schema subset."""


class GenerationError(RiffError):
    """No valid fixture was found within the configured generation budget."""


class ProviderError(RiffError):
    """A user provider raised an exception or returned a non-JSON value."""
