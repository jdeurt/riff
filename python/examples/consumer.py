"""Run a vendor-consumer smoke example with ordinary HTTPX interception.

Run from python/: uv run python -m examples.consumer
"""

from pathlib import Path

import httpx

from examples.models import Customer
from riff import FixtureFactory


def customer_label(client: httpx.Client, customer_id: str) -> str:
    """Normalize a vendor name, falling back to its stable customer identifier."""
    response = client.get(f"https://vendor.example/customers/{customer_id}")
    response.raise_for_status()
    customer = Customer.model_validate_json(response.content, strict=True)
    return customer.name.strip() or customer.id


def main() -> None:
    """Feed generated responses through the HTTP client and actual consumer."""
    factory = FixtureFactory.from_file(
        Path(__file__).resolve().parents[2] / "examples/vendor.yaml",
        path="/customers/{id}",
        fixture_file=Path(__file__).resolve().parents[2] / "examples/vendor.fixtures.yaml",
    )
    for body in factory.generate(count=50, seed=42):
        transport = httpx.MockTransport(
            lambda _request, fixture=body: httpx.Response(200, json=fixture)
        )
        with httpx.Client(transport=transport) as client:
            label = customer_label(client, "test-customer")
            if not label:
                message = "Consumer returned an empty customer label."
                raise RuntimeError(message)


if __name__ == "__main__":
    main()
