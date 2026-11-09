"""Typed fixture generation from schemas, OpenAPI, and models."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Literal, TypeVar

from pydantic import BaseModel, TypeAdapter

from riff._native import NativeFactory
from riff.compiler import JSON_ADAPTER, SCHEMA_ADAPTER
from riff.contracts import FailureCase, FixtureCase, SuccessCase
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
SUCCESSES = TypeAdapter[list[SuccessCase[JSONValue]]](list[SuccessCase[JSONValue]])


class FixtureFactory:
    """Compile a contract once and generate independent, seeded batches."""

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
        self._configure(
            {"kind": "inline", "value": schema}, providers=providers, fixture_file=fixture_file
        )

    def _configure(
        self,
        source: dict[str, JSONValue],
        *,
        providers: Mapping[str, Provider] | None,
        fixture_file: str | Path | None,
        operation: dict[str, JSONValue] | None = None,
    ) -> None:
        self._providers = dict(providers or {})
        configuration: dict[str, JSONValue] = {
            "providers": list(self._providers),
            "operation": operation,
            "fixtures": None
            if fixture_file is None
            else {"kind": "file", "value": str(fixture_file)},
        }
        self._native = NativeFactory(
            json.dumps(JSON_ADAPTER.validate_python(source), allow_nan=False),
            json.dumps(configuration, allow_nan=False),
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
        """Select an exact OpenAPI request or response body."""
        factory = cls.__new__(cls)
        operation: dict[str, JSONValue] = {
            "path": path,
            "method": method,
            "status": status,
            "mediaType": media_type,
            "body": body,
        }
        cls._configure(
            factory,
            {"kind": "inline", "value": document},
            providers=providers,
            fixture_file=fixture_file,
            operation=operation,
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
        factory = cls.__new__(cls)
        operation: dict[str, JSONValue] | None = None
        if path is not None:
            operation = {
                "path": path,
                "method": method,
                "body": body,
                "status": status,
                "mediaType": media_type,
            }
        cls._configure(
            factory,
            {"kind": "file", "value": str(source)},
            providers=providers,
            fixture_file=fixture_file,
            operation=operation,
        )
        return factory

    @classmethod
    def for_model(
        cls, model: type[ModelT], *, providers: Mapping[str, Provider] | None = None
    ) -> ModelFactory[ModelT]:
        """Infer validated output models from the caller's Pydantic class."""
        return cls.from_schema(
            SCHEMA_ADAPTER.validate_python(model.model_json_schema()), providers=providers
        ).as_model(model)

    def as_model(self, model: type[ModelT]) -> ModelFactory[ModelT]:
        """Bind a typed view while retaining original payloads and schema validation."""
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
        options = _encode_options(count, seed, profile, max_attempts)
        return VALUES.validate_json(
            self._native.generate(options, provider_bridge(self._providers))
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
        options = _encode_options(count, seed, profile, max_attempts, "pass")
        return SUCCESSES.validate_json(
            self._native.cases(options, provider_bridge(self._providers))
        )

    def failure_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FailureCase]:
        """Mutate declared failures, retaining raw values and diagnostics."""
        options = _encode_options(count, seed, profile, max_attempts, "fail")
        return FAILURES.validate_json(self._native.cases(options, provider_bridge(self._providers)))

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
        options = _encode_options(count, seed, profile, max_attempts, expectation)
        output = self._native.cases(options, provider_bridge(self._providers))
        if expectation == "pass":
            return list(SUCCESSES.validate_json(output))
        return list(FAILURES.validate_json(output))

    def minimize(
        self,
        value: JSONValue,
        *,
        is_failure: Callable[[JSONValue], bool],
        max_evaluations: int = 100,
        preserve_schema: bool = True,
    ) -> MinimizationResult:
        """Reduce a reproducer using a caller-owned failure oracle."""
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


def _encode_options(
    count: int,
    seed: int | None,
    profile: Profile,
    max_attempts: int,
    expectation: Expectation | None = None,
) -> str:
    if seed is not None and (isinstance(seed, bool) or not isinstance(seed, int)):
        message = "seed must be an integer or None"
        raise ValueError(message)
    controls: dict[str, JSONValue] = {
        "count": count,
        "seed": seed,
        "profile": profile,
        "maxAttempts": max_attempts,
    }
    if expectation is not None:
        controls["expectation"] = expectation
    return json.dumps(controls, allow_nan=False)
