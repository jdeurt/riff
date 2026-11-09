"""Hide inline fixture annotations in FastAPI's public OpenAPI document."""

from fastapi import FastAPI
from fastapi.openapi.utils import get_openapi
from pydantic import TypeAdapter

from riff import JSONValue, strip_fixture_metadata


def configure_public_openapi(app: FastAPI) -> None:
    """Keep cached public documentation separate from model-generation metadata."""
    cached: dict[str, JSONValue] | None = None
    adapter = TypeAdapter[dict[str, JSONValue]](dict[str, JSONValue])

    def public_openapi() -> dict[str, JSONValue]:
        nonlocal cached
        if cached is None:
            original = adapter.validate_python(
                get_openapi(title=app.title, version=app.version, routes=app.routes)
            )
            cached = strip_fixture_metadata(original)
        return cached

    app.openapi = public_openapi
