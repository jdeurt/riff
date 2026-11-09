"""Riff supplies values, HTTPX supplies transports, and pytest checks outcomes."""

import asyncio
from pathlib import Path

import httpx
import pytest

from examples.app import create_app
from examples.models import Customer, Problem, Welcome
from riff import FailureCase, FixtureFactory, JSONValue, SuccessCase

FACTORY = FixtureFactory.from_file(
    Path(__file__).resolve().parents[2] / "examples/endpoint-vendor.yaml", path="/customers/{id}"
).as_model(Customer)


async def call_welcome(body: JSONValue) -> httpx.Response:
    """Feed fixture data into an independent HTTPX mock and call the real app."""
    transport = httpx.MockTransport(lambda _request: httpx.Response(200, json=body))
    with httpx.Client(transport=transport) as vendor:
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=create_app(vendor)), base_url="http://testserver"
        ) as client:
            return await client.get("/customers/test-customer/welcome")


@pytest.mark.parametrize("case", FACTORY.success_cases(count=20, seed=42))
def test_successful_customer_welcome(case: SuccessCase[Customer]) -> None:
    body = case.value.model_dump(mode="json")
    response = asyncio.run(call_welcome(body))
    assert response.status_code == 200
    welcome = Welcome.model_validate_json(response.content)
    assert welcome.message == f"Hello, {case.value.name.strip() or case.value.id}!"


@pytest.mark.parametrize("case", FACTORY.failure_cases(count=30, seed=42))
def test_expected_customer_failures(case: FailureCase) -> None:
    response = asyncio.run(call_welcome(case.value))
    expected = {
        "/name": (502, "Malformed vendor customer"),
        "/status": (409, "Customer is inactive"),
    }
    status, detail = expected[case.target]
    assert response.status_code == status
    assert Problem.model_validate_json(response.content).detail == detail
