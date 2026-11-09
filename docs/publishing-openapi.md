# Publish OpenAPI without fixture annotations

A [sidecar fixture file](sidecars.md) keeps metadata out of the source OpenAPI
document entirely. When using inline annotations such as `Field(json_schema_extra=
fixture(...))`, remove them from the copy your application publishes.

For FastAPI, customize its `openapi` function, following the framework's
[custom OpenAPI pattern](https://fastapi.tiangolo.com/how-to/extending-openapi/):

```python
from fastapi import FastAPI
from fastapi.openapi.utils import get_openapi
from pydantic import TypeAdapter
from riff import JSONValue, strip_fixture_metadata


def configure_public_openapi(app: FastAPI) -> None:
    cached: dict[str, JSONValue] | None = None
    adapter = TypeAdapter[dict[str, JSONValue]](dict[str, JSONValue])

    def public_openapi() -> dict[str, JSONValue]:
        nonlocal cached
        if cached is None:
            original = adapter.validate_python(
                get_openapi(
                    title=app.title,
                    version=app.version,
                    routes=app.routes,
                ),
            )
            cached = strip_fixture_metadata(original)
        return cached

    app.openapi = public_openapi
```

Call `configure_public_openapi(app)` after registering the application's routes.
Its `/openapi.json`, Swagger UI, and ReDoc use the sanitized cached document.
The [runnable hook](../python/examples/published_openapi.py) and
[HTTP workflow test](../python/tests/test_annotations_publication.py) verify this
against FastAPI.

`strip_fixture_metadata` returns a fresh OpenAPI copy. It removes `x-fixture`
annotations from schema locations, including nested properties and compositions.
It preserves validation constraints, references, other extensions, and literal
example/default data. A payload property actually named `x-fixture` remains a
property; its name is not confused with the annotation keyword.

Your Pydantic models keep their metadata, so `FixtureFactory.for_model(...)`
continues working. Keep an authoring/private contract or a sidecar when generating
fixtures from OpenAPI: the public copy intentionally no longer carries the hints.
The utility operates on schema structure independently of generation limits and
supports both OpenAPI 3.0 and 3.1 publication.
