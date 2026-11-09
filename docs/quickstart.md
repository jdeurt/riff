# Quick start

Riff supplies JSON data to your tests. It does not send requests or intercept
network traffic. Choose a schema, generate a batch, and feed it into your usual
test client or mock transport.

## Install from the repository

```sh
git clone https://github.com/jdeurt/riff.git
cd riff/python
uv sync --locked
```

For another uv project, install the repository's Python package:

```sh
uv add 'riff @ git+https://github.com/jdeurt/riff.git#subdirectory=python'
```

## Generate vendor responses

```python
from riff import FixtureFactory

factory = FixtureFactory.from_file(
    "../examples/vendor.yaml", path="/customers/{id}"
)
fixtures = factory.generate(count=20, seed=42)
```

Each value is a vendor response body. Pass it to HTTPX, responses, respx, a VCR
fixture adapter, or your own test fixture layer.

```sh
uv run riff ../examples/vendor.yaml --path '/customers/{id}' --count 5 --seed 42
uv run riff ../examples/vendor.yaml --path '/customers/{id}' \
  --expectation fail --count 5 --seed 42 --jsonl
```

The first command writes a JSON array of bodies. The second writes one labeled
failure case per line. Fail labels come from explicit `x-fixture.fail` metadata.

## Generate your own API requests

```python
factory = FixtureFactory.from_file(
    "../examples/own-api.yaml", path="/registrations", method="post", body="request"
)
for case in factory.generate_cases(count=20, seed=42):
    response = client.post("/registrations", json=case.value)
    assert response.status_code == 201
```

`client` is your application's test client. Run the complete examples with:

```sh
uv run pytest examples
```

## Use typed successful values

```python
from pydantic import BaseModel, Field
from riff import FixtureFactory

class Registration(BaseModel):
    name: str = Field(min_length=1, max_length=64)
    seats: int = Field(ge=1, le=50)

factory = FixtureFactory.for_model(Registration)
for case in factory.success_cases(count=20, seed=42):
    assert 1 <= case.value.seats <= 50
    response = client.post("/registrations", json=case.value.model_dump())
    assert response.status_code == 201
```

The factory infers `Registration`; the value is validated before your test
receives it. To retain a vendor's OpenAPI contract, load it normally and call
`.as_model(YourResponseModel)`. Use `.failure_cases()` for raw intentionally
failing values. See [endpoint testing](testing.md) for working examples.
