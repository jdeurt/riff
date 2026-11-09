# Python API

Python code lives in `python/`. Shared schemas and documentation live at the
repository root. Run development commands from `python/` with uv.

## FixtureFactory

- `from_schema(schema, providers=None)`: compile a standalone Draft 7 subset.
- `from_openapi(document, *, path, method="get", status="200", media_type="application/json", body="response", providers=None)`: select an exact request or response body schema.
- `from_file(source, *, path=None, ...)`: read local JSON/YAML; a path selects an OpenAPI operation, otherwise the file is a standalone schema.
- `for_model(Model, providers=None)`: derive a schema from a Pydantic model and return a model-typed factory.
- `as_model(Model)`: bind an existing compiled contract to a Pydantic model.
- `generate(*, count=1, seed=None, profile="mixed", max_attempts=100)`: return a list of schema-valid JSON bodies.
- `generate_cases(*, count=1, expectation="pass", seed=None, profile="mixed", max_attempts=100)`: return tagged success/failure cases.
- `minimize(value, *, is_failure, max_evaluations=100, preserve_schema=True)`: reduce a known failing JSON value using the caller's oracle.

Typed factories expose `generate()` for validated models, `success_cases()` for
`SuccessCase[Model]`, and `failure_cases()` for raw `FailureCase` values.
Model binding checks both the original schema and the model. Successful fixtures
are validated, not coerced into model validity. Malformed failure values never
pass through successful model decoding.

## Types and errors

`JSONValue` is the recursive JSON union. `Provider` is a callable receiving a
`ProviderContext`, whose fields are `seed`, `locale`, `schema`, and `options`.
`SuccessCase[T]` has a typed value and literal `pass` / true outcome fields.
`FailureCase` has raw JSON, a literal `fail` expectation, observed `schema_valid`,
`violations`, a JSON Pointer `target`, and optional `reason`. `FixtureCase` is their
union. Each `Violation` has `path` and `message`.

`SchemaError` identifies invalid or unsupported contracts. `GenerationError`
identifies exhausted search, decoding failure, or generation resource limits.
`ProviderError` identifies provider exceptions or non-JSON results. All inherit
`RiffError`. Invalid invocation options raise `ValueError`. Oracle exceptions in
minimization propagate to the caller.

## Python schema subset and limits

Supported validation keywords are `type`, `enum`, `const`, `minimum`, `maximum`,
`exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `minLength`, `maxLength`,
`pattern`, `format`, `properties`, `required`, `additionalProperties`,
`patternProperties`, `minProperties`, `maxProperties`, `items`, `minItems`,
`maxItems`, `uniqueItems`, `allOf`, `anyOf`, `oneOf`, `not`, and `contains`.
Boolean schemas and acyclic local references are supported. Definitions are
reference targets. All results are validated against the original normalized
schema; compositions and negative constraints use bounded candidate search.

String formats: `email`, `uuid`, `uri`, `hostname`, `ipv4`, `ipv6`, `date`, and
`date-time`. Numeric formats `int32`, `int64`, `float`, and `double` are annotations;
use numeric bounds to enforce a width. Regex generation uses Python-compatible
patterns through rstr; unsupported constructs may fail generation. No cross-engine
regex equivalence is promised.

OpenAPI 3.1 is accepted only within this subset. Newer keywords such as
`prefixItems`, `unevaluatedProperties`, and dynamic references are rejected, as
are external/recursive references, reference siblings, and unknown formats.
`nullable` is accepted only while normalizing OpenAPI 3.0.

Structural generation limits depth to 32 and string/collection sizes to 1024.
Unspecified size maxima use 32 characters or 5 array items. Each fixture has a
shared candidate budget of `100 * max_attempts`, in addition to per-node retry
budgets. Explicit values and provider output are validated but are not resized.
These budgets do not time-limit providers, regex engines, or failure oracles.

The CLI writes arrays by default, JSON Lines with `--jsonl`, and case envelopes
when `--expectation pass|fail` is supplied. It writes diagnostics to stderr and
returns exit code 2 on contract/generation failures, with no partial JSON output.
It never imports custom providers from schema files.
