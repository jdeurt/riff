# Riff

Schema-guided fuzz fixtures for API tests. Generate request bodies for your own
endpoints or response bodies for code that consumes another API.

Give Riff a JSON Schema or OpenAPI document, then supply the generated data through
your existing test client, interceptor, mock transport, or VCR setup. Riff does
not send or intercept requests.

Use built-in semantic providers or register custom ones. Combine strategies,
filter candidates, declare expected failures, replay saved fixtures, and minimize
reproducers. Successful fixtures respect the schema. Explicit failure strategies
can violate constraints or represent schema-valid business failures.

Fixture hints may live inline or in YAML sidecars selected by endpoint and schema
path. Python and JavaScript provide typed model views while retaining the exact
JSON payloads used by your tests.

## Guides

Start with a quick start, then choose model annotations or a YAML sidecar.

- [Python quick start](docs/quickstart.md)
- [JavaScript quick start and API](docs/javascript-api.md)
- [Typed model metadata](docs/model-metadata.md)
- [Sidecar fixture files](docs/sidecars.md)
- [Testing your own endpoints and vendor consumers](docs/testing.md)
- [Strategies, filtering, and minimization](docs/strategies.md)
- [Custom providers](docs/providers.md)
- [JSON Schema constraints and input limits](docs/json-schema.md)
- [Publishing a clean OpenAPI spec](docs/publishing-openapi.md)
- [Fuzzing Riff’s input contracts](docs/input-fuzzing.md)
- [Python API reference](docs/python-api.md)
- [Installation and development](docs/installation.md)
- [Strict tooling](docs/tooling.md)
- [Portable specification](docs/spec.md)

Python requires 3.10+; JavaScript requires Node 22.12+. Follow the
[installation guide](docs/installation.md) to build from this repository.
