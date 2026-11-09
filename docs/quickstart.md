# Quick start

Riff generates JSON data for your tests. Start with a schema or a typed model,
then give the resulting payloads to your existing test client or HTTP mock.

## Install

```sh
git clone https://github.com/jdeurt/riff.git
cd riff/python
uv sync --locked
```

Run the examples below from `python/`. In another uv project, install the package
with:

```sh
uv add 'riff @ git+https://github.com/jdeurt/riff.git#subdirectory=python'
```

## Generate your first fixtures

```python
from riff import FixtureFactory

factory = FixtureFactory.from_schema(
    {"type": "integer", "minimum": 1, "maximum": 50},
)
values = factory.generate(count=20, seed=42)
assert all(isinstance(value, int) and 1 <= value <= 50 for value in values)
```

Generation respects constraints by default. A fixed seed reproduces a batch in
the same pinned environment. Riff raises an error if it cannot find valid values
within its budget.

## Test your own endpoint with typed values

For an endpoint that accepts this registration and returns HTTP 201, use your
application's existing test client:

```python
from pydantic import BaseModel, Field
from riff import Failure, FixtureFactory, fixture

class Registration(BaseModel):
    name: str = Field(min_length=1, max_length=64)
    seats: int = Field(
        ge=1,
        le=50,
        json_schema_extra=fixture(fail=Failure(examples=[0, 51])),
    )

factory = FixtureFactory.for_model(Registration)
for case in factory.success_cases(count=20, seed=42):
    assert 1 <= case.value.seats <= 50
    response = client.post("/registrations", json=case.payload)
    assert response.status_code == 201

for case in factory.failure_cases(count=20, seed=42):
    response = client.post("/registrations", json=case.payload)
    assert response.status_code == 422
```

`client` belongs to your test framework. `case.value` is a validated
`Registration` for success cases; `case.payload` retains the original JSON,
including extra fields allowed by the schema. Failure payloads remain raw because
0 and 51 deliberately violate the seat constraint.

The [complete endpoint examples](testing.md) run without a live server:

```sh
uv run pytest examples
```

## Start from OpenAPI instead

Load a request body from the [example API contract](../examples/own-api.yaml):

```python
factory = FixtureFactory.from_file(
    "../examples/own-api.yaml",
    path="/registrations",
    method="post",
    body="request",
)
cases = factory.failure_cases(count=20, seed=42)
```

The CLI writes data for any tool that consumes JSON:

```sh
uv run riff ../examples/own-api.yaml --path /registrations --method post \
  --body request --expectation fail --count 5 --seed 42 --jsonl
```

## Supply vendor responses

Select a response body and attach a separate fixture file:

```python
factory = FixtureFactory.from_file(
    "../examples/vendor.yaml",
    path="/customers/{id}",
    fixture_file="../examples/vendor.fixtures.yaml",
)
responses = factory.generate(count=20, seed=42)
```

Give each body to HTTPX, responses, respx, your VCR fixture layer, or another mock.
Riff supplies data; it does not intercept or record requests.

See [typed metadata](model-metadata.md) or [sidecars](sidecars.md) to configure
sources, [strategies](strategies.md) for filtering and deliberate violations,
and [public OpenAPI](publishing-openapi.md) to hide inline annotations.
