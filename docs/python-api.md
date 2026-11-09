# Python API reference

Python code lives in `python/`; shared schemas and guides live at the repository
root. Run development commands from `python/` with uv.

## Construct a factory

| Operation | Input and result |
| --- | --- |
| `FixtureFactory.from_schema(schema, ...)` | Compile a standalone schema in a supported dialect. |
| `FixtureFactory.from_openapi(document, ...)` | Select one operation's exact request or response body. |
| `FixtureFactory.from_file(source, ...)` | Read local JSON/YAML; select OpenAPI when `path` is supplied. |
| `FixtureFactory.for_model(Model, ...)` | Derive the contract from a Pydantic model and infer its result type. |
| `factory.as_model(Model)` | Bind a typed result model while retaining an existing contract. |

Schema/file/OpenAPI constructors accept `providers` and an optional `fixture_file`.
Provider registrations map names to `Provider` callables. The fixture file uses
the [sidecar format](sidecars.md). Model-first construction accepts `providers`;
use field metadata to attach its fixture hints.

OpenAPI selection options are keyword-only:

| Option | Default | Meaning |
| --- | --- | --- |
| `path` | Required for OpenAPI | Exact path template from the document. |
| `method` | `"get"` | Operation method; selection is case-insensitive. |
| `body` | `"response"` | `"request"` selects the request body. |
| `status` | `"200"` | Exact response status; ignored for requests. |
| `media_type` | `"application/json"` | Exact media type in the content map. |

`from_file` treats a file as a standalone schema when `path=None`.

## Generate data

Every generation method accepts `count=1`, `seed=None`, `profile="mixed"`, and
`max_attempts=100` as keyword-only arguments. Count is nonnegative; attempt limits
are positive integers. Profiles are `mixed`, `realistic`, and `boundary`.

| Method | Raw `FixtureFactory` | Typed `ModelFactory[T]` |
| --- | --- | --- |
| `generate(...)` | `list[JSONValue]` | `list[T]` |
| `success_cases(...)` | `list[SuccessCase[JSONValue]]` | `list[SuccessCase[T]]` |
| `failure_cases(...)` | `list[FailureCase]` | `list[FailureCase]` |

Raw factories additionally provide `generate_cases(..., expectation="pass")`.
Set `expectation="fail"` to get explicitly declared negative cases. Prefer the
separate success/failure methods when their concrete result types are useful.

Typed factories validate both the source payload and model decoding. They also
check the model's serialization against the original schema, rejecting bindings
that discard required data. Successful models are not coerced into validity.
Malformed failure values do not go through successful model decoding.

## Cases and JSON types

`JSONValue` is the recursive union of JSON scalars, arrays, and string-keyed
objects. Numbers must be finite.

`SuccessCase[T]` contains a typed `value`, the original JSON `payload`, and literal
`expectation="pass"` / `schema_valid=True` fields. Send `payload` through HTTP tools:
it retains permitted extra keys even when a model ignores unknown fields.

`FailureCase` exposes raw JSON as both `value` and `payload`. It has a literal
`expectation="fail"`, observed `schema_valid`, `violations`, a JSON Pointer `target`,
and optional `reason`. `FixtureCase` is the union of raw success and failure cases.
Each `Violation` contains `path` and `message`.

A fail label declares expected application behavior. A schema-valid inactive
customer and a malformed customer can both be expected failures. Your test owns
the assertions.

Failure declarations follow value-bearing schemas, including applicable branches,
extra-property schemas, and item/contains schemas. To test invalid property names,
put a failure example or violation strategy on the containing object. Failure
annotations on name/content-description predicates do not rename keys or decode
embedded content.

## Metadata, providers, and publication

`fixture(...)` builds the typed `x-fixture` mapping for `json_schema_extra`.
`Failure(...)` declares its negative source. Programmatic strategies are
`SchemaStrategy`, `ValuesStrategy`, `ProviderStrategy`, `ChoiceStrategy`, and
`FilterStrategy.where(strategy, schema=...)`. See [typed metadata](model-metadata.md).

`Provider` is a callable receiving `ProviderContext`. Its fields are a per-call
`seed`, `locale`, normalized `schema`, and provider-specific `options`. The result
is one JSON value. See [custom providers](providers.md) for registration and
isolation rules.

`strip_fixture_metadata(openapi_document)` returns a clean publication copy.
The [FastAPI publication guide](publishing-openapi.md) shows its cached OpenAPI hook.

## Minimize a reproducer

Raw factories provide:

```python
result = factory.minimize(
    value,
    is_failure=oracle,
    max_evaluations=100,
    preserve_schema=True,
)
```

`oracle` accepts JSON and returns a boolean. It must report failure for the
original value. `MinimizationResult` contains `value`, `evaluations`, and
`budget_exhausted`. Schema preservation defaults to true; disable it for malformed
cases. Oracle exceptions propagate. See [minimization](strategies.md#reduce-a-failing-value).

## Errors and limits

`SchemaError` identifies invalid or unsupported contracts. `GenerationError`
identifies exhausted search, model decoding/serialization failures, or resource
limits. `ProviderError` identifies custom-provider exceptions or non-JSON output.
All inherit `RiffError`. Invalid invocation options raise `ValueError`.

Validation supports Draft 7, 2019-09, and 2020-12 (the default), including OpenAPI
3.1 dialect inheritance. The [JSON Schema guide](json-schema.md) lists vocabulary,
format behavior, and exclusions. Candidate generation is bounded; it does not
solve every satisfiable combination or prove unsatisfiability on exhaustion.

Structural generation limits depth to 32 and string/collection sizes to 1024.
Unspecified maxima prefer 32-character strings and 5-item arrays. Open objects
explore up to two extra keys beyond their declared property count. Each valid-body
draw has a shared candidate budget of `100 * max_attempts`,
with per-node retry limits too. A negative-source draw starts a separate budget;
failure-case construction retries at most `max_attempts` base/replacement attempts.
Its aggregate work can therefore exceed a single draw budget.
Explicit values/provider output are validated but not resized. Budgets do not
time-limit provider code, regex engines, or failure oracles.

## CLI

The CLI writes JSON arrays by default, JSON Lines with `--jsonl`, and case
envelopes with `--expectation pass|fail`. `--fixtures file.yaml` attaches a sidecar;
`--body request --method post` selects request data. It never imports provider code
from schemas. Diagnostics go to stderr; contract/generation failures return exit
code 2 without partial JSON output.

The [dogfooding guide](dogfooding.md) describes the run-options schema and generated
contracts used to test these operations.
