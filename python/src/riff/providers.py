"""Default semantic data providers; public names are independent of Faker."""

from __future__ import annotations

from datetime import UTC, date, datetime, timedelta
from typing import TYPE_CHECKING

from faker import Faker

from riff.errors import SchemaError

if TYPE_CHECKING:
    from random import Random


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


def make_faker(locale: str, random: Random) -> Faker:
    """Create a provider whose random state belongs only to this batch."""
    faker = Faker(locale.replace("-", "_"))
    faker.seed_instance(random.getrandbits(64))
    return faker


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
            start = datetime(2000, 1, 1, tzinfo=UTC)
            return (start + timedelta(seconds=random.randrange(946771200))).isoformat()
        case _:
            message = f"Unsupported string format: {format_name}"
            raise SchemaError(message)
