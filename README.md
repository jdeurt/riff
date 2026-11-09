# Riff

Schema-guided fuzz fixtures for API tests. Generate request bodies for your own
endpoints or response bodies for code that consumes another API.

Riff produces data. Your existing test runner, HTTP client, interceptor, mock
transport, or VCR setup supplies that data to the system under test.

The [specification](docs/spec.md) is language-agnostic. The first implementation
lives in [`python/`](python/), with Faker-backed defaults, custom providers,
composable strategies, labeled success/failure cases, and seeded replay.
Fixture annotations may live inline or in sidecar YAML files selected by endpoint
and schema path. Default generation respects constraints; explicit failure
strategies can violate them.

## Guides

- [Quick start](docs/quickstart.md)
- [Sidecar fixture files](docs/sidecars.md)
- [Dogfooding Riff](docs/dogfooding.md)
- [Strategies, filtering, and minimization](docs/strategies.md)
- [Custom providers](docs/providers.md)
- [Testing your own endpoints and vendor consumers](docs/testing.md)
- [Python API](docs/python-api.md)
- [Portable specification](docs/spec.md)

## Python development

Python 3.12+ and [uv](https://docs.astral.sh/uv/) are required.

```sh
cd python
uv sync --locked
uv run ruff check .
uv run ruff format --check .
uv run ty check
uv run pytest tests examples
uv build
```

Successful fixtures are validated at runtime and can be returned as typed models.
Intentionally failing fixtures retain raw JSON types. Python's type checker does
not prove schema constraints; it checks the modeled API and case distinctions.

The Python implementation forbids unchecked type assertions and unconstrained
dynamic types. External JSON/YAML is validated at input boundaries; internal code
uses typed schema and strategy representations. Ruff enables all lint rules with
narrow, documented exceptions; `ty` treats all diagnostics as errors.
