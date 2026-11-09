"""Typed Python facade for the shared Rust fixture engine."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Literal, TypeVar

from faker.config import AVAILABLE_LOCALES
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, ValidationError

from riff._native import NativeFactory
from riff.compiler import JSON_ADAPTER, SCHEMA_ADAPTER
from riff.contracts import FailureCase, FixtureCase, SuccessCase
from riff.documents import decode_native, read_document
from riff.errors import SchemaError
from riff.minimize import MinimizationResult, minimize
from riff.models import ModelFactory
from riff.providers import provider_bridge
from riff.types import JSONValue, Profile

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping
    from pathlib import Path

    from riff.contracts import Provider
    from riff.types import Expectation, Schema

ModelT = TypeVar("ModelT", bound=BaseModel)
VALUES = TypeAdapter[list[JSONValue]](list[JSONValue])
FAILURES = TypeAdapter[list[FailureCase]](list[FailureCase])


class GenerationOptions(BaseModel):
    """Validate invocation options before calling native code."""

    model_config = ConfigDict(strict=True, extra="forbid")
    count: int = Field(ge=0, le=10000)
    seed: int | None
    profile: Profile
    max_attempts: int = Field(ge=1, le=100000)


class FixtureFactory:
    """Compile a contract once; generate independent, seeded native batches."""

    _native: NativeFactory
    _providers: dict[str, Provider]

    def __init__(
        self,
        schema: Schema,
        *,
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> None:
        """Compile a standalone schema with optional registered providers and sidecar."""
        self._configure(schema, providers=providers, fixture_file=fixture_file)

    def _configure(
        self,
        document: JSONValue,
        *,
        providers: Mapping[str, Provider] | None,
        fixture_file: str | Path | None,
        operation: dict[str, JSONValue] | None = None,
    ) -> None:
        self._providers = dict(providers or {})
        try:
            copied = JSON_ADAPTER.validate_python(document)
        except ValidationError as error:
            message = f"Contract must contain only finite JSON values: {error}"
            raise SchemaError(message) from error
        overlay = (
            None
            if fixture_file is None
            else json.dumps(read_document(fixture_file), allow_nan=False)
        )
        self._native = NativeFactory(
            json.dumps(copied, allow_nan=False),
            list(self._providers),
            [locale.replace("_", "-") for locale in AVAILABLE_LOCALES],
            None if operation is None else json.dumps(operation),
            overlay,
        )

    @classmethod
    def from_schema(
        cls,
        schema: Schema,
        *,
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Compile a standalone Draft 7, 2019-09, or 2020-12 schema."""
        return cls(schema, providers=providers, fixture_file=fixture_file)

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
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Select an exact OpenAPI request or response body, resolved by Rust."""
        factory = cls.__new__(cls)
        operation: dict[str, JSONValue] = {
            "path": path,
            "method": method,
            "status": status,
            "mediaType": media_type,
            "body": body,
        }
        cls._configure(
            factory, document, providers=providers, fixture_file=fixture_file, operation=operation
        )
        return factory

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
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Read local JSON/YAML and optionally select an OpenAPI body."""
        document = read_document(source)
        if path is not None:
            if not isinstance(document, dict):
                message = "OpenAPI document must be an object."
                raise SchemaError(message)
            return cls.from_openapi(
                document,
                path=path,
                method=method,
                status=status,
                media_type=media_type,
                body=body,
                providers=providers,
                fixture_file=fixture_file,
            )
        if not isinstance(document, (dict, bool)):
            message = "Standalone schema must be an object or boolean."
            raise SchemaError(message)
        return cls.from_schema(document, providers=providers, fixture_file=fixture_file)

    @classmethod
    def for_model(
        cls, model: type[ModelT], *, providers: Mapping[str, Provider] | None = None
    ) -> ModelFactory[ModelT]:
        """Infer validated output models from the caller's Pydantic class."""
        return cls.from_schema(
            SCHEMA_ADAPTER.validate_python(model.model_json_schema()), providers=providers
        ).as_model(model)

    def as_model(self, model: type[ModelT]) -> ModelFactory[ModelT]:
        """Bind a typed view, retaining original payloads and native validation."""
        return ModelFactory(self, model)

    def generate(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[JSONValue]:
        """Return a complete valid batch; never return partial data on exhaustion."""
        options = self._options(count, seed, profile, max_attempts)
        return VALUES.validate_python(
            decode_native(self._native.generate(options, provider_bridge(self._providers)))
        )

    def success_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[SuccessCase[JSONValue]]:
        """Return positive cases with literal pass/valid types."""
        return [
            SuccessCase(value=value, payload=value)
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
        """Mutate declared failures in Rust, retaining raw values and diagnostics."""
        options = self._options(count, seed, profile, max_attempts)
        return FAILURES.validate_python(
            decode_native(self._native.failure_cases(options, provider_bridge(self._providers)))
        )

    def generate_cases(
        self,
        *,
        count: int = 1,
        expectation: Expectation = "pass",
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FixtureCase]:
        """Keep caller-declared outcome separate from observed schema validity."""
        if expectation == "pass":
            return list(
                self.success_cases(
                    count=count, seed=seed, profile=profile, max_attempts=max_attempts
                )
            )
        if expectation == "fail":
            return list(
                self.failure_cases(
                    count=count, seed=seed, profile=profile, max_attempts=max_attempts
                )
            )
        message = f"Unknown case expectation: {expectation}"
        raise ValueError(message)

    def minimize(
        self,
        value: JSONValue,
        *,
        is_failure: Callable[[JSONValue], bool],
        max_evaluations: int = 100,
        preserve_schema: bool = True,
    ) -> MinimizationResult:
        """Reduce a reproducer using the native engine and caller-owned oracle."""
        return minimize(
            value,
            self._native,
            is_failure=is_failure,
            max_evaluations=max_evaluations,
            preserve_schema=preserve_schema,
        )

    def is_valid(self, value: JSONValue) -> bool:
        """Check JSON against the original contract without generating candidates."""
        payload = JSON_ADAPTER.validate_python(value)
        return self._native.is_valid(json.dumps(payload, allow_nan=False))

    @staticmethod
    def _options(count: int, seed: int | None, profile: Profile, max_attempts: int) -> str:
        options = GenerationOptions(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return json.dumps(
            {
                "count": options.count,
                "seed": None if options.seed is None else str(options.seed % (1 << 64)),
                "profile": options.profile,
                "maxAttempts": options.max_attempts,
            }
        )
