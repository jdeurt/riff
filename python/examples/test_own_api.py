"""Test your own API using OpenAPI-first and model-first request fixtures."""

import asyncio
from pathlib import Path

import httpx
import pytest

from examples.app import create_app
from examples.models import Registration, Reservation
from riff import FailureCase, FixtureFactory, JSONValue, SuccessCase

OPENAPI = FixtureFactory.from_file(
    Path(__file__).resolve().parents[2] / "examples/own-api.yaml",
    path="/registrations",
    method="post",
    body="request",
).as_model(Registration)
MODEL = FixtureFactory.for_model(Registration)


async def post_registration(body: JSONValue) -> httpx.Response:
    """Use an ASGI test transport without any external API requests."""
    with httpx.Client() as vendor:
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=create_app(vendor)), base_url="http://testserver"
        ) as client:
            return await client.post("/registrations", json=body)


@pytest.mark.parametrize(
    "case", [*OPENAPI.success_cases(count=15, seed=7), *MODEL.success_cases(count=15, seed=7)]
)
def test_registration_accepts_valid_bodies_and_prices_seats(
    case: SuccessCase[Registration],
) -> None:
    response = asyncio.run(post_registration(case.payload))
    assert response.status_code == 201
    reservation = Reservation.model_validate_json(response.content)
    assert reservation.name == case.value.name
    assert reservation.total_cents == case.value.seats * 1200


@pytest.mark.parametrize(
    "case", [*OPENAPI.failure_cases(count=15, seed=7), *MODEL.failure_cases(count=15, seed=7)]
)
def test_registration_rejects_declared_invalid_bodies(case: FailureCase) -> None:
    response = asyncio.run(post_registration(case.value))
    assert response.status_code == 422
