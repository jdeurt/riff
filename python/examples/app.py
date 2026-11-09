"""An application with both its own request contract and a vendor dependency."""

import httpx
from fastapi import FastAPI, HTTPException
from pydantic import ValidationError

from examples.models import Customer, Registration, Reservation, Welcome


def create_app(vendor: httpx.Client) -> FastAPI:
    """Accept an ordinary HTTP client; the host test owns transport interception."""
    app = FastAPI()

    @app.get("/customers/{customer_id}/welcome")
    def welcome(customer_id: str) -> Welcome:
        """Greet active customers and reject malformed or inactive records."""
        response = vendor.get(f"https://vendor.example/customers/{customer_id}")
        response.raise_for_status()
        try:
            customer = Customer.model_validate_json(response.content, strict=True)
        except ValidationError as error:
            raise HTTPException(status_code=502, detail="Malformed vendor customer") from error
        if customer.status != "active":
            raise HTTPException(status_code=409, detail="Customer is inactive")
        return Welcome(message=f"Hello, {customer.name.strip() or customer.id}!")

    @app.post("/registrations", status_code=201)
    def register(request: Registration) -> Reservation:
        """Reserve seats at the documented price of 1200 cents per seat."""
        return Reservation(name=request.name, total_cents=request.seats * 1200)

    return app
