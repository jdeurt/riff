"""Default semantic data providers; public names are independent of Faker."""

from __future__ import annotations

import json
from datetime import date, datetime, timedelta, timezone
from random import Random
from typing import TYPE_CHECKING, Literal

from faker import Faker
from pydantic import BaseModel, ConfigDict, Field

from riff.compiler import JSON_ADAPTER
from riff.contracts import ProviderContext
from riff.errors import ProviderError, SchemaError
from riff.types import JSONValue

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from riff.contracts import Provider


STRING_FORMATS = frozenset(
    {
        "email",
        "idn-email",
        "uuid",
        "uri",
        "uri-reference",
        "iri",
        "iri-reference",
        "uri-template",
        "hostname",
        "idn-hostname",
        "ipv4",
        "ipv6",
        "date",
        "date-time",
        "time",
        "duration",
        "json-pointer",
        "relative-json-pointer",
        "regex",
        "color",
    }
)


def semantic_value(provider: str, faker: Faker) -> str:
    """Generate a portable provider value without exposing Faker method names."""
    generators = {
        "email": faker.email,
        "full-name": faker.name,
        "company-name": faker.company,
        "url": faker.url,
        "phone-number": faker.phone_number,
        "city": faker.city,
        "country-code": faker.country_code,
        "uuid": faker.uuid4,
    }
    return generators[provider]()


def formatted_value(format_name: str, faker: Faker, random: Random) -> str:
    """Generate supported formats with fixed time bounds for reproducibility."""
    generators = {
        "idn-email": faker.email,
        "idn-hostname": faker.hostname,
        "uri-reference": faker.url,
        "iri": faker.url,
        "iri-reference": faker.url,
        "email": faker.email,
        "uuid": faker.uuid4,
        "uri": faker.url,
        "hostname": faker.hostname,
        "ipv4": faker.ipv4,
        "ipv6": faker.ipv6,
    }
    if format_name in generators:
        return generators[format_name]()
    fixed_formats = {
        "json-pointer": "/name",
        "relative-json-pointer": "0/name",
        "uri-template": "/customers/{id}",
        "regex": "[A-Z]+",
        "color": "#a0b1c2",
    }
    if format_name in fixed_formats:
        return fixed_formats[format_name]
    match format_name:
        case "time":
            return (
                f"{random.randrange(24):02d}:{random.randrange(60):02d}:{random.randrange(60):02d}Z"
            )
        case "duration":
            return f"P{random.randrange(366)}D"
        case "date":
            return (date(2000, 1, 1) + timedelta(days=random.randrange(10958))).isoformat()
        case "date-time":
            start = datetime(2000, 1, 1, tzinfo=timezone.utc)
            return (start + timedelta(seconds=random.randrange(946771200))).isoformat()
        case _:
            message = f"Unsupported string format: {format_name}"
            raise SchemaError(message)


class NativeProviderRequest(BaseModel):
    """Validated native ABI request; decimal seed strings retain all 64 bits."""

    model_config = ConfigDict(strict=True, extra="forbid")
    kind: Literal["provider", "format"]
    name: str
    seed: str
    locale: str
    field_schema: dict[str, JSONValue] = Field(alias="schema")
    options: dict[str, JSONValue]


def provider_bridge(providers: Mapping[str, Provider]) -> Callable[[str], str]:
    """Create a per-batch callback adapting native requests to Python providers."""
    fakers: dict[str, Faker] = {}

    def invoke(payload: str) -> str:
        request = NativeProviderRequest.model_validate_json(payload)
        seed = int(request.seed)
        try:
            custom = providers.get(request.name)
            if request.kind == "provider" and custom is not None:
                value = custom(
                    ProviderContext(
                        seed=seed,
                        locale=request.locale,
                        schema=request.field_schema,
                        options=request.options,
                    )
                )
            else:
                faker = fakers.get(request.locale)
                if faker is None:
                    faker = Faker(request.locale.replace("-", "_"))
                    fakers[request.locale] = faker
                faker.seed_instance(seed)
                format_random = Random(seed)  # noqa: S311 -- Reproducible test fixtures.
                value = (
                    formatted_value(request.name, faker, format_random)
                    if request.kind == "format"
                    else semantic_value(request.name, faker)
                )
            return json.dumps(JSON_ADAPTER.validate_python(value), allow_nan=False)
        except Exception as error:
            message = f"Provider {request.name!r} failed: {error}"
            raise ProviderError(message) from error

    return invoke
