# JavaScript and TypeScript

Generate API fixtures in JavaScript or TypeScript with the same schemas and
sidecars used by Python. The package source is
[`javascript/`](../javascript/), with runnable
[endpoint](../javascript/tests/endpoints.test.ts) and
[workflow](../javascript/tests/workflows.test.ts) examples.

## Install from this repository

Install [Rust](https://rustup.rs/), Node 22.12+, and npm. From `javascript/`:

```sh
npm ci
npm run check
npm pack
npm run check:package
```

Install the resulting `riff-fixtures-0.1.0.tgz` into a Node project:

```sh
npm install /path/to/riff/javascript/riff-fixtures-0.1.0.tgz
# Install Zod separately to run the model example below.
npm install zod
```

Build the package on its target operating system and architecture. This package
has not been published to an npm registry.

## Typed endpoint fixtures

Zod 4 implements Standard Schema and Standard JSON Schema. Riff infers its output
type through those interfaces, without coupling the public API to Zod:

```typescript
import { z } from "zod";
import { FixtureFactory, fixture } from "riff-fixtures";

const Registration = z.strictObject({
  name: z.string().min(1).max(64).meta(fixture({ provider: "full-name" })),
  seats: z.number().int().min(1).max(50).meta(
    fixture({
      fail: { strategy: { kind: "schema", constraints: "violate" } },
    }),
  ),
});

const factory = FixtureFactory.forModel(Registration);
for (const caseValue of factory.successCases({ count: 20, seed: 42n })) {
  const seats: number = caseValue.value.seats;
  const response = await client.post("/registrations", {
    json: caseValue.payload,
  });
  assert.equal(response.status, 201);
}
for (const caseValue of factory.failureCases({ count: 20, seed: 42n })) {
  const response = await client.post("/registrations", {
    json: caseValue.payload,
  });
  assert.equal(response.status, 422);
}
```

`client` and `assert` belong to your test tools. Successful `value` fields are
inferred validated views; `payload` retains the generated JSON, including allowed
extra fields. Failure values stay raw. Runtime validation still enforces numeric
bounds and other constraints: TypeScript's `number` type cannot prove `1..50`.

Use `fromSchema(...).asModel(model)` when a separately supplied JSON Schema is
authoritative. Model validation must be synchronous. Riff checks the model's
JSON serialization against that original schema too.

## Factories and options

| Operation | Purpose |
| --- | --- |
| `FixtureFactory.fromSchema(schema, options?)` | Standalone schema; defaults to Draft 2020-12. |
| `FixtureFactory.fromOpenAPI(document, operation, options?)` | Exact request/response selection. |
| `FixtureFactory.fromFile(path, options?)` | Local JSON/YAML; `path` in options selects OpenAPI. |
| `FixtureFactory.forModel(model, options?)` | Infer schema and typed output through Standard Schema interfaces. |
| `factory.asModel(model)` | Bind an existing contract to a validated typed view. |
| `factory.isValid(value)` | Validate JSON using the compiled original contract. |

Factory options are `providers`, `fixtures` (an inline sidecar document), or
`fixtureFile` (its local filename). Choose one sidecar source.

```typescript
const factory = FixtureFactory.fromFile("vendor.yaml", {
  path: "/customers/{id}",
  method: "get",
  body: "response",
  status: "200",
  mediaType: "application/json",
  fixtureFile: "vendor.fixtures.yaml",
});
```

The [sidecar syntax](sidecars.md) uses RFC 6901 schema pointers after reference
expansion. Request selection ignores `status`. Missing exact responses and
ambiguous operation overlays fail compilation.

`generate`, `successCases`, and `failureCases` accept a plain options object:

| Option | Default | Meaning |
| --- | --- | --- |
| `count` | `1` | Integer from 0 through 10,000. |
| `seed` | Fresh randomness | Safe integer `number` or `bigint` up to 1024 decimal digits, normalized modulo 2^64. |
| `profile` | `"mixed"` | `"mixed"`, `"realistic"`, or `"boundary"`. |
| `maxAttempts` | `100` | Per-node rejection limit, 1 through 100,000. |

Raw factories also provide `generateCases({ expectation: "pass" | "fail", ... })`.
Success tags are literal `expectation: "pass"` and `schema_valid: true` types.
Failures carry `target`, `reason`, and `violations`, and report observed
`schema_valid` separately from the expected consumer outcome. Portable case
envelope fields retain their snake_case spelling.

## Custom providers

```typescript
import type { Provider } from "riff-fixtures";

const accountId: Provider = (context) => `acct_${context.seed.toString(16)}`;
const factory = FixtureFactory.fromSchema(
  {
    type: "string",
    "x-fixture": { provider: "acme.account-id" },
  },
  { providers: { "acme.account-id": accountId } },
);
```

Providers return one JSON value and may generate entire objects or arrays.
Their context contains `seed: bigint`, `locale`, normalized `schema`, and
`options`. Contexts are fresh and isolated. Return finite, plain JSON data;
promises, cyclic objects, accessors, class instances, and non-JSON values fail.
Registrations can override built-in names. All callbacks execute synchronously.

Built-in locale identifiers are `en-US`, `en-GB`, `de-DE`, `fr-FR`, and `ja-JP`.
Built-in locale availability is checked at compilation; custom providers decide
which locales and options they support. Matching versions and seeds produce
identical built-in data in Python and JavaScript for their common JSON values.
See the portable
[provider contract](providers.md) and [replay rules](spec.md#9-budgets-reproducibility-and-replay).

## Minimize and publish

```typescript
const result = factory.minimize(failingPayload, {
  isFailure: oracle,
  maxEvaluations: 100,
  preserveSchema: true,
});
```

The synchronous oracle must report failure for the initial value. Its exceptions
propagate unchanged. Results contain `value`, `evaluations`, and
`budgetExhausted`. Disable schema preservation for malformed inputs. Candidate
work is bounded independently of oracle calls.
Minimization options must also be a plain object. Arrays, class instances, and
unknown control fields are rejected.

`stripFixtureMetadata(openapi)` returns a clean publication copy. It preserves
constraints, references, examples, defaults, and payload properties literally
named `x-fixture`.

## Errors, limits, and CLI

`SchemaError`, `GenerationError`, and `ProviderError` inherit `RiffError`.
Provider failures retain the original exception as `cause`. Invalid control
values raise `RangeError`; incompatible argument types can raise `TypeError`.
Failed model validation or schema-incompatible model serialization raises
`GenerationError`. Model code that throws an exception propagates it unchanged.
The shared [schema guide](json-schema.md) documents constraints and resource
limits. Numeric JSON that cannot round-trip through JavaScript is rejected;
generation never silently rounds a valid fixture.

```sh
node dist/src/cli.js ../examples/own-api.yaml --path /registrations \
  --method post --body request --expectation fail --count 5 --seed 42 --jsonl
```

The CLI uses only built-in providers, writes diagnostics to stderr, and exits
with code 2 without partial JSON on failure. It supplies data to your HTTP tools;
it does not intercept, record, or send requests.
