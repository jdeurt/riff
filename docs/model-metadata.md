# Typed model metadata

Use `fixture(...)` when adding Riff metadata to a Pydantic field. It returns the
complete `x-fixture` mapping, with typed arguments and early source validation.

```python
from pydantic import BaseModel, ConfigDict, Field
from riff import Failure, FixtureFactory, SchemaStrategy, fixture

class Registration(BaseModel):
    model_config = ConfigDict(strict=True)
    name: str = Field(
        min_length=1,
        max_length=64,
        json_schema_extra=fixture(
            provider="full-name",
            fail=Failure(examples=["", None]),
        ),
    )
    seats: int = Field(
        ge=1,
        le=50,
        json_schema_extra={
            **fixture(
                fail=Failure(
                    strategy=SchemaStrategy(kind="schema", constraints="violate"),
                ),
            ),
        },
    )

factory = FixtureFactory.for_model(Registration)
```

Pass `fixture(...)` directly, or spread it into `json_schema_extra` when combining
it with other extensions. Do not wrap its result in another `x-fixture` key.

Provider shorthand accepts `provider`, `locale`, `options`, and `examples`.
Alternatively pass `strategy`; the typed overloads keep it separate from those
shorthand arguments. `fail` accepts a typed `Failure` with the same source fields
and an optional `reason`. Provider names are resolved when the factory is compiled,
so custom names work exactly as they do in YAML.

## Programmatic strategies

`SchemaStrategy`, `ValuesStrategy`, `ProviderStrategy`, and `ChoiceStrategy` build
the same portable trees as their YAML counterparts. Use `FilterStrategy.where`
to supply an ordinary schema predicate with a typed strategy:

```python
from riff import FilterStrategy, ValuesStrategy, fixture

metadata = fixture(
    strategy=FilterStrategy.where(
        ValuesStrategy(kind="values", values=[-1, 10, 11]),
        schema={"type": "integer", "minimum": 10},
    ),
)
```

All class discriminators use the same `kind` names as the spec. The builder
validates metadata combinations immediately. Schema compatibility of explicit
examples, provider registrations, and generated values is checked during factory
compilation or generation.

## Typed view and original payload

`factory.success_cases()` returns `SuccessCase[Registration]`. Access typed fields
through `case.value.name` and `case.value.seats`. Send `case.payload` to a JSON test
client. It retains the exact generated body, including extra properties permitted
by the schema even if the model ignores them.

`factory.generate()` returns validated models directly. Model binding checks the
model's serialization against the original schema and rejects bindings that drop
required data. If testing unknown-field behavior, use cases and their original
payloads rather than reserializing the model view.

`factory.failure_cases()` returns raw `FailureCase` values. Their payloads may be
malformed by design, so representing them as valid registration models would be
incorrect. Both case types have `payload`; negative cases also expose that same
JSON as `value`.

See [the runnable models](../python/examples/models.py) and
[own-API tests](../python/examples/test_own_api.py). To hide inline metadata from
public documentation, use [the publication guide](publishing-openapi.md), or put
annotations in [a sidecar file](sidecars.md).
