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
other constraints still apply. Regex generation uses Python-compatible syntax
through rstr, with no cross-engine equivalence guarantee.

Local acyclic JSON Pointer references are expanded against the supplied document.
Remote references, recursive/dynamic references, anchor references, reference
siblings, custom vocabularies, and URI resource/base-ID scoping are outside the
reference implementation. Unsupported schema validation keywords are rejected.
Use an explicit provider or explicit values when generation needs domain rules
beyond the contract.

## Deliberate violations

`constraints: respect` is the default for a schema strategy. In an explicit
failure declaration, `constraints: violate` derives negative boundaries such as
0/51 for an integer bounded from 1 to 50, or extra keys for a closed object.
Custom failure providers and explicit failure examples can produce other cases.
The enclosing case separately reports whether the full body satisfies its schema.
