"""Type-safe construction of portable fixture metadata for model fields."""

from __future__ import annotations

from typing import TYPE_CHECKING, overload

from riff.compiler import JSON_ADAPTER, FixtureMetadata, Strategy
from riff.compiler import FailureSource as Failure

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from riff.types import JSONValue

__all__ = ["Failure", "fixture"]


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
) -> dict[str, JSONValue]: ...


def fixture(
    *,
    provider: str | None = None,
    locale: str | None = None,
    options: Mapping[str, JSONValue] | None = None,
    examples: Sequence[JSONValue] | None = None,
    strategy: Strategy | None = None,
    fail: Failure | None = None,
) -> dict[str, JSONValue]:
    """Build json_schema_extra data, validating incompatible metadata eagerly.

    Pass the result directly to a model field, or spread it into a larger mapping.
    Provider names are resolved later against the factory's registration mapping.
    """
    metadata: dict[str, JSONValue] = {}
    if provider is not None:
        metadata["provider"] = provider
    if locale is not None:
        metadata["locale"] = locale
    if options is not None:
        metadata["options"] = dict(options)
    if examples is not None:
        metadata["examples"] = list(examples)
    if strategy is not None:
        metadata["strategy"] = JSON_ADAPTER.validate_python(
            strategy.model_dump(mode="json", by_alias=True, exclude_unset=True)
        )
    if fail is not None:
        metadata["fail"] = JSON_ADAPTER.validate_python(
            fail.model_dump(mode="json", by_alias=True, exclude_unset=True)
        )
    parsed = FixtureMetadata.model_validate(metadata)
    return {
        "x-fixture": JSON_ADAPTER.validate_python(
            parsed.model_dump(mode="json", by_alias=True, exclude_unset=True)
        )
    }
