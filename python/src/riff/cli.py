"""Generate payloads or labeled cases without importing provider code."""

from __future__ import annotations

import argparse
import json
import sys
from typing import TYPE_CHECKING, Literal

from pydantic import BaseModel, ConfigDict

from riff.contracts import case_json
from riff.errors import RiffError
from riff.factory import FixtureFactory
from riff.types import Expectation, Profile

if TYPE_CHECKING:
    from riff.types import JSONValue


class Arguments(BaseModel):
    """Validate the argparse boundary once and retain concrete option types."""

    model_config = ConfigDict(strict=True)
    source: str
    path: str | None
    method: str
    status: str
    media_type: str
    count: int
    seed: int | None
    profile: Profile
    max_attempts: int
    jsonl: bool
    expectation: Expectation | None
    body: Literal["request", "response"]
    fixtures: str | None


def main() -> int:
    """Write complete JSON results to stdout and errors to stderr."""
    parser = argparse.ArgumentParser(description="Generate schema-guided API fuzz fixtures.")
    parser.add_argument("source", help="Local JSON or YAML schema / OpenAPI document")
    parser.add_argument("--path", help="OpenAPI path template")
    parser.add_argument("--method", default="get")
    parser.add_argument("--status", default="200")
    parser.add_argument("--media-type", default="application/json")
    parser.add_argument("--body", choices=("request", "response"), default="response")
    parser.add_argument("--fixtures", help="Sidecar fixture YAML file")
    parser.add_argument("--count", type=int, default=1)
    parser.add_argument("--seed", type=int)
    parser.add_argument("--profile", choices=("mixed", "realistic", "boundary"), default="mixed")
    parser.add_argument("--max-attempts", type=int, default=100)
    parser.add_argument("--jsonl", action="store_true")
    parser.add_argument("--expectation", choices=("pass", "fail"), help="Write labeled cases")
    try:
        arguments = Arguments.model_validate(vars(parser.parse_args()))
        factory = FixtureFactory.from_file(
            arguments.source,
            path=arguments.path,
            method=arguments.method,
            status=arguments.status,
            media_type=arguments.media_type,
            body=arguments.body,
            fixture_file=arguments.fixtures,
        )
        fixtures: list[JSONValue]
        if arguments.expectation is None:
            fixtures = factory.generate(
                count=arguments.count,
                seed=arguments.seed,
                profile=arguments.profile,
                max_attempts=arguments.max_attempts,
            )
        else:
            fixtures = [
                case_json(case)
                for case in factory.generate_cases(
                    count=arguments.count,
                    seed=arguments.seed,
                    profile=arguments.profile,
                    max_attempts=arguments.max_attempts,
                    expectation=arguments.expectation,
                )
            ]
    except (RiffError, ValueError) as error:
        print(f"riff: {error}", file=sys.stderr)  # noqa: T201 -- CLI error channel.
        return 2
    if arguments.jsonl:
        for fixture in fixtures:
            print(json.dumps(fixture, ensure_ascii=True, allow_nan=False))  # noqa: T201
    else:
        print(json.dumps(fixtures, indent=2, ensure_ascii=True, allow_nan=False))  # noqa: T201
    return 0
