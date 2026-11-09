"""Compile contracts and expose complete fixture-generation operations."""

from __future__ import annotations

from pathlib import Path
from secrets import randbits
from typing import TYPE_CHECKING, Literal

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError

from riff.cases import generate_cases, generate_failure_cases
from riff.compiler import JSON_ADAPTER, compile_schema
from riff.contracts import FailureCase, FixtureCase, Provider, SuccessCase
from riff.engine import Engine
from riff.errors import SchemaError
from riff.minimize import MinimizationResult, minimize
from riff.models import ModelFactory
from riff.overlays import OperationSelector, apply_fixture_file
from riff.schema import dereference_object, normalize_schema
from riff.types import Profile

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from riff.types import Expectation, JSONValue, Schema


class GenerationOptions(BaseModel):
    """Validate invocation options once, before the engine receives them."""

    model_config = ConfigDict(strict=True)
    count: int = Field(ge=0)
    seed: int | None
    profile: Profile
    max_attempts: int = Field(ge=1)


class FixtureFactory:
    """Compile a contract once and generate independent, seeded fixture batches."""

    def __init__(
        self,
        schema: Schema,
        *,
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> None:
        """Compile a standalone contract; named constructors also support OpenAPI."""
        try:
            document = JSON_ADAPTER.validate_python(schema)
        except ValidationError as error:
            message = f"Contract must contain only finite JSON values: {error}"
            raise SchemaError(message) from error
        self._contract = normalize_schema(document, document, openapi30=False)
        if fixture_file is not None:
            self._contract = apply_fixture_file(self._contract, fixture_file, selector=None)
        self._providers = dict(providers or {})
        self._schema = compile_schema(self._contract, self._providers)

    @classmethod
    def from_schema(
        cls,
        schema: Schema,
        *,
        providers: Mapping[str, Provider] | None = None,
        fixture_file: str | Path | None = None,
    ) -> FixtureFactory:
        """Compile a standalone schema in the supported Draft 7 subset."""
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
        """Select an exact request or response body from OpenAPI 3.0/3.1."""
        try:
            copied = JSON_ADAPTER.validate_python(document)
        except ValidationError as error:
            message = f"OpenAPI input must contain only finite JSON values: {error}"
            raise SchemaError(message) from error
        if not isinstance(copied, dict):
            message = "OpenAPI document must be an object."
            raise SchemaError(message)
        version = copied.get("openapi")
        if not isinstance(version, str) or not version.startswith(("3.0.", "3.1.")):
            message = "Expected an OpenAPI 3.0 or 3.1 document."
            raise SchemaError(message)
        try:
            paths = dereference_object(copied, copied["paths"])
            path_item = dereference_object(copied, paths[path])
            operation = dereference_object(copied, path_item[method.lower()])
            if body == "request":
                selected = dereference_object(copied, operation["requestBody"])
            elif body == "response":
                responses = dereference_object(copied, operation["responses"])
                selected = dereference_object(copied, responses[status])
            else:
                message = f"Unknown body selection: {body}"
                raise ValueError(message)
            content = dereference_object(copied, selected["content"])
            media = dereference_object(copied, content[media_type])
            schema = media["schema"]
        except KeyError as error:
            message = (
                f"{body.title()} schema not found for {method.upper()} {path}, "
                f"{status}, {media_type}."
            )
            raise SchemaError(message) from error
        normalized = normalize_schema(schema, copied, openapi30=version.startswith("3.0."))
        if fixture_file is not None:
            selector = OperationSelector(
                path=path, method=method, status=status, body=body, mediaType=media_type
            )
            normalized = apply_fixture_file(normalized, fixture_file, selector=selector)
        return cls(normalized, providers=providers)

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
        """Read local JSON/YAML, optionally selecting an OpenAPI operation body."""
        try:
            with Path(source).open(encoding="utf-8") as stream:
                document = JSON_ADAPTER.validate_python(yaml.safe_load(stream))
        except (OSError, UnicodeError, yaml.YAMLError, ValidationError) as error:
            message = f"Unable to read contract {source}: {error}"
            raise SchemaError(message) from error
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
    def for_model[ModelT: BaseModel](
        cls, model: type[ModelT], *, providers: Mapping[str, Provider] | None = None
    ) -> ModelFactory[ModelT]:
        """Infer the result type and contract from a Pydantic model."""
        document = JSON_ADAPTER.validate_python(model.model_json_schema())
        if not isinstance(document, dict):
            message = "Model did not produce a schema object."
            raise SchemaError(message)
        return cls.from_schema(document, providers=providers).as_model(model)

    def as_model[ModelT: BaseModel](self, model: type[ModelT]) -> ModelFactory[ModelT]:
        """Bind a validated result model while retaining the original contract."""
        return ModelFactory(self, model, self._contract)

    def generate(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[JSONValue]:
        """Return valid JSON bodies or raise, without returning a partial batch."""
        engine = self._new_engine(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return engine.generate(self._schema, count=count)

    def success_cases(
        self,
        *,
        count: int = 1,
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[SuccessCase[JSONValue]]:
        """Return successful payloads with statically known outcome and validity."""
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
        """Replace one declared field and retain raw types for malformed values."""
        engine = self._new_engine(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return generate_failure_cases(self._schema, engine, count=count)

    def generate_cases(
        self,
        *,
        count: int = 1,
        expectation: Expectation = "pass",
        seed: int | None = None,
        profile: Profile = "mixed",
        max_attempts: int = 100,
    ) -> list[FixtureCase]:
        """Generate declared outcomes, keeping observed schema validity separate."""
        engine = self._new_engine(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return generate_cases(self._schema, engine, count=count, expectation=expectation)

    def minimize(
        self,
        value: JSONValue,
        *,
        is_failure: Callable[[JSONValue], bool],
        max_evaluations: int = 100,
        preserve_schema: bool = True,
    ) -> MinimizationResult:
        """Reduce a reproducer using a caller-owned oracle and finite budget."""
        return minimize(
            value,
            self._contract,
            is_failure=is_failure,
            max_evaluations=max_evaluations,
            preserve_schema=preserve_schema,
        )

    def _new_engine(
        self, *, count: int, seed: int | None, profile: Profile, max_attempts: int
    ) -> Engine:
        options = GenerationOptions(
            count=count, seed=seed, profile=profile, max_attempts=max_attempts
        )
        return Engine(
            seed=randbits(64) if options.seed is None else options.seed,
            profile=options.profile,
            max_attempts=options.max_attempts,
            providers=self._providers,
        )
