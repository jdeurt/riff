# Testing endpoints and API consumers

Use the schema for the body you are supplying, then let your existing test tools
execute the operation. Riff does not replace a network interceptor or VCR.

## Your own API

[`own-api.yaml`](../examples/own-api.yaml) declares the request body for
`POST /registrations`. Successful values obey the contract. Declared failure
sources exercise invalid names and out-of-range seat counts.

The complete [own-API example](../python/examples/test_own_api.py) generates
bodies and sends them through the application's test client. It asserts HTTP 201
and returned data for successful bodies, and HTTP 422 for malformed bodies.

```python
factory = FixtureFactory.from_file(
    "../examples/own-api.yaml",
    path="/registrations",
    method="post",
    body="request",
)
for case in factory.generate_cases(count=30, seed=42, expectation="fail"):
    response = client.post("/registrations", json=case.value)
    assert response.status_code == 422
```

For a model-first application, `FixtureFactory.for_model(RequestModel)` returns
validated request models from `.generate()` and typed cases from
`.success_cases()`. Send `case.payload` through your client; it retains generated
extra fields. The supplied example covers both OpenAPI-first and model-first
generation.

The JavaScript [registration handler](../javascript/examples/registration.ts)
uses Zod's inferred request type and a standard Fetch `Request`/`Response` API.
Its [tests](../javascript/tests/endpoints.test.ts) send generated success and
failure payloads through the handler and check the independent 1200-cent price
rule. The vendor example in the same suite runs a real local HTTP server before
passing its response through the consumer's parser. From `javascript/`, run
`npm test` to build and execute both workflows.

## An endpoint that calls a vendor

The [vendor-consumer example](../python/examples/test_endpoint.py) uses three
independent layers:

1. Riff supplies a JSON body from [the vendor contract](../examples/endpoint-vendor.yaml).
2. HTTPX's mock transport returns that body at the vendor HTTP boundary.
3. The test calls the real application endpoint and checks its status and body.

Successful fixtures select active customers. Name failures replace a string with
null or a number, and the application must return HTTP 502. Status failures set
`inactive`, which remains schema-valid, and the application must return HTTP 409.
The test's expected statuses are application requirements, not inferred from Riff.

Typed successful cases expose fields through `case.value`; their original JSON
is `case.payload`, including permitted extra keys. Failure cases stay raw because
malformed values cannot honestly be represented as a validated customer model.
If you inspect raw fields, narrow them at that test boundary before using them.

## Run and replay

From `python/`:

```sh
uv run pytest examples
```

Use a fixed seed for a stable CI batch; vary seeds in additional runs to explore
more inputs. Save failing payloads and their case envelopes. Pinning the runtime
and `uv.lock` supports seeded replay; retained JSON supports replay after upgrades.
Riff does not promise exhaustive sampling, automatic scheduling, or reproduction
from a seed across different implementation/dependency versions.


Riff also [fuzzes its JSON Schema, OpenAPI, and sidecar inputs](input-fuzzing.md).
Those tests run in the Rust package.
