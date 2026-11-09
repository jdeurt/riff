"""Typed successful values without misrepresenting malformed negative fixtures."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

from jsonschema import Draft7Validator, FormatChecker
from pydantic import BaseModel, ValidationError

from riff.compiler import JSON_ADAPTER
from riff.contracts import FailureCase, SuccessCase
from riff.errors import GenerationError

if TYPE_CHECKING:
    from riff.factory import FixtureFactory
    from riff.types import JSONValue, Profile, Schema


class ModelFactory[ModelT: BaseModel]:
    """Bind a fixture contract to a model, preserving its inferred result type."""

    def __init__(self, factory: FixtureFactory, model: type[ModelT], schema: Schema) -> None:
        """Retain the compiled factory and its caller-selected result model."""
        self._factory = factory
        self._model = model
        self._validator = Draft7Validator(schema, format_checker=FormatChecker())

    def generate(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[ModelT]:
        """Validate each successful payload into its model before returning it."""
        values = self._factory.generate(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return [self._decode(value) for value in values]

    def success_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[SuccessCase[ModelT]]:
        """Return model-typed values in positively tagged case envelopes."""
        return [
            SuccessCase(value)
            for value in self.generate(
                count=count, seed=seed, profile=profile, max_attempts=max_attempts
            )
        ]

    def failure_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FailureCase]:
        """Keep negative payloads as raw JSON; do not decode them into valid models."""
        return self._factory.failure_cases(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )

    def _decode(self, value: JSONValue) -> ModelT:
        try:
            model = self._model.model_validate_json(json.dumps(value, allow_nan=False), strict=True)
        except ValidationError as error:
            message = f"Generated value violates bound model {self._model.__name__}: {error}"
            raise GenerationError(message) from error
        serialized = JSON_ADAPTER.validate_python(
            model.model_dump(mode="json", by_alias=True, exclude_unset=True, round_trip=True)
        )
        if not self._validator.is_valid(serialized):
            message = f"Serialized bound model {self._model.__name__} violates the original schema."
            raise GenerationError(message)
        return model
