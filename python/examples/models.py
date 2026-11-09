"""Application models also provide typed fixture contracts."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class Customer(BaseModel):
    """Fields consumed by the welcome operation."""

    model_config = ConfigDict(strict=True)
    id: str
    name: str = Field(max_length=100)
    status: Literal["active", "inactive"]


class Registration(BaseModel):
    """A registration reserves 1-50 seats at 1200 cents per seat."""

    model_config = ConfigDict(strict=True)
    name: str = Field(
        min_length=1,
        max_length=64,
        json_schema_extra={
            "x-fixture": {"provider": "full-name", "fail": {"examples": ["", None]}}
        },
    )
    seats: int = Field(
        ge=1, le=50, json_schema_extra={"x-fixture": {"fail": {"examples": [0, 51, "many"]}}}
    )


class Reservation(BaseModel):
    """The endpoint's calculated total and customer name."""

    name: str
    total_cents: int


class Welcome(BaseModel):
    """The welcome endpoint's successful response body."""

    message: str


class Problem(BaseModel):
    """The welcome endpoint's application error response body."""

    detail: str
