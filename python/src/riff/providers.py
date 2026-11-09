"""Adapt registered Python callbacks to the fixture provider contract."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

from pydantic import BaseModel, ConfigDict, Field

from riff.compiler import JSON_ADAPTER
from riff.contracts import ProviderContext
from riff.errors import ProviderError
from riff.types import JSONValue

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from riff.contracts import Provider


class ProviderRequest(BaseModel):
    """Typed callback request; decimal seed strings retain every seed bit."""

    model_config = ConfigDict(strict=True)
    name: str
    seed: str
    locale: str
    field_schema: dict[str, JSONValue] = Field(alias="schema")
    options: dict[str, JSONValue]


def provider_bridge(providers: Mapping[str, Provider]) -> Callable[[str], str]:
    """Call a registered provider with an isolated, typed context."""

    def invoke(payload: str) -> str:
        request = ProviderRequest.model_validate_json(payload)
        try:
            value = providers[request.name](
                ProviderContext(
                    seed=int(request.seed),
                    locale=request.locale,
                    schema=request.field_schema,
                    options=request.options,
                )
            )
            return json.dumps(JSON_ADAPTER.validate_python(value), allow_nan=False)
        except Exception as error:
            message = f"Provider {request.name!r} failed: {error}"
            raise ProviderError(message) from error

    return invoke
