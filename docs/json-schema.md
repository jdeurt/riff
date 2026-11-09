# JSON Schema constraints and extra data

Riff validates every successful fixture with the selected JSON Schema dialect.
Generation uses targeted hints and bounded rejection; validation is the final
gate. A difficult satisfiable schema can still exhaust generation. Explicit
values or a custom provider can supply candidates for those cases.

## Dialects

Standalone schemas default to 2020-12. `$schema` selects Draft 7, 2019-09, or
2020-12. OpenAPI 3.1's `jsonSchemaDialect` is inherited by body schemas, and a
schema's own `$schema` takes precedence. OpenAPI 3.0 nullable types and boolean
exclusive bounds are normalized before generation.

Keywords inactive in the selected dialect do not become generation requirements.
For example, Draft 7 uses `dependencies`; modern schemas use `dependentRequired`
and `dependentSchemas`. Draft 7 and 2019-09 tuples use an `items` array with
`additionalItems`; 2020-12 uses `prefixItems` with `items`.

## Validation vocabulary

| Values | Constraints |
| --- | --- |
| Any JSON value | `type`, `const`, `enum`, `allOf`, `anyOf`, `oneOf`, `not`, `if` / `then` / `else` |
| Numbers | Inclusive/exclusive bounds and `multipleOf` |
| Strings | Length, `pattern`, recognized `format` assertions |
| Objects | `properties`, `required`, `patternProperties`, `additionalProperties`, `propertyNames`, property counts, dependencies, `unevaluatedProperties` |
| Arrays | Item/prefix/tuple schemas, sizes, uniqueness, `contains`, `minContains`, `maxContains`, `unevaluatedItems` |

The independent validator enforces these constraints, including applicable
composition scopes. Generation has targeted handling for common combinations;
it is not a complete constraint solver. Negations, mutually restrictive patterns,
and complex overlapping compositions can require supplied examples/providers or
more attempts. Exhaustion raises `GenerationError`, never an invalid success case.

Descriptions, defaults, read/write annotations, discriminators, and `content*`
keywords remain annotations. Riff does not infer business rules, remove read-only
fields, or decode content merely from an annotation.

## Additional properties

For an object with a declared `known` property:

- `additionalProperties: false`: no unmatched properties are generated.
- `additionalProperties: true`, or omission: batches explore extra keys and
  arbitrary JSON values, including nested garbage data.
- A schema-valued `additionalProperties`: generated extra values must satisfy
  that schema, such as bounded integers or strings matching a pattern.

Pattern-matched properties are validated by every matching `patternProperties`
schema. They are not forbidden merely because `additionalProperties` is false.
Property-name rules and `unevaluatedProperties` also participate in validation.

The default object size preference explores up to two keys beyond the declared
property count, subject to schema limits and the candidate budget. Extra keys
are sampled, not forced into every fixture: omissions and empty objects also
matter. `maxProperties` and closed/unevaluated rules can restrict this exploration.

Typed models may ignore allowed unknown fields. Use a success case's `payload`
when sending data through a test client; it retains extra keys while `value`
provides typed access to known fields.

## Formats and references

Recognized standard formats are asserted using the selected dialect's format
checker. Built-in generators cover email/IDN email, hostnames, IPv4/IPv6, UUID,
URI/IRI references, URI templates, date/time/duration, JSON Pointers, and regex
strings. An unknown format remains an annotation, as JSON Schema specifies;
other constraints still apply. Regex validation supports backtracking constructs;
pattern sampling supports a smaller set. Supply explicit examples or a provider
when Riff cannot sample a pattern. Sampling limits patterns to 1024 UTF-8 bytes,
repetitions to 32, and expansion capacity to 4096. Regex semantics can differ
from your application's validator.

Local acyclic JSON Pointer references are expanded against the supplied document.
Remote references, recursive/dynamic references, anchor references, reference
siblings, custom vocabularies, and URI resource/base-ID scoping are unsupported in
this release. Nested dialect switching is also unsupported. Unsupported schema
validation keywords are rejected.
Use an explicit provider or explicit values when generation needs domain rules
beyond the contract.

## Deliberate violations

`constraints: respect` is the default for a schema strategy. In an explicit
failure declaration, `constraints: violate` derives negative boundaries such as
0/51 for an integer bounded from 1 to 50. Unsupported local boundary mutations
fall back to type violations; use explicit failure values to exercise extra keys
on a closed object.
Custom failure providers and explicit failure examples can produce other cases.
The enclosing case separately reports whether the full body satisfies its schema.

## Numbers and limits

Numeric constraints preserve their JSON decimal meaning, including integers
larger than 64 bits. Numeric coefficients are limited to 1024 digits. Absolute decimal scales must
remain within 1024 before and after normalization; oversized values and extreme
exponents are rejected. Numeric JSON text also has a 2100-character lexical limit.
These checks apply to contracts, explicit values, provider results, validation
inputs, and minimization candidates.

Python exposes integers and finite floats. JavaScript JSON values must use finite
numbers, with integer values and schema bounds limited to ±(2⁵³−1);
use `bigint` for seeds, which are separate from JSON payloads. File readers reject
numeric literals that those public types would round. Generated values that
cannot be represented raise `GenerationError`; the fixture is never silently
altered. A number already rounded before being passed to
Riff cannot be recovered; use strings for domain values needing decimal fidelity.

Pass and fail payloads have maximum depth 32 and string/collection sizes 1024,
including explicit values and provider results. Contract input nesting is limited
to 128 and resolved schema structure to 16384 nodes. Generation has a shared
candidate-work budget of `100 * maxAttempts` per draw, plus per-node rejection
limits. Count is limited to 10000 and attempts to 100000. These limits bound
search work; provider code, oracle code, and regex execution have no timeout.

Minimization enumerates reductions lazily. Its candidate-work budget is
`100 * maxEvaluations`, separate from the oracle-call limit; either can set the
result's exhaustion flag. It does not allocate all possible reductions upfront.

## Contract files

Both languages read UTF-8 JSON and YAML 1.2 using the same rules. Files are limited
to 16 MiB and must contain one document with string object keys and finite JSON
values. Duplicate keys, YAML aliases, and explicit tags are rejected. Quote
numeric response statuses and strings such as `'null'` or `'true'` when those
words are data. Strings must contain valid Unicode scalar values; unpaired
surrogates are rejected. JSON files must also satisfy JSON syntax.

Numeric literals retain their original meaning. A file value that the receiving
language would round is rejected before fixture generation.
