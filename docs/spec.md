# Riff portable specification, draft 0.1

This document specifies fixture data and observable behavior independently of a
programming language or generation backend. **MUST**, **SHOULD**, and **MAY** are
requirements, recommendations, and optional capabilities within this draft.

## 1. Purpose and vocabulary

A **fixture** is a JSON value supplied to a test. A **schema** constrains those
values. A **strategy** describes how candidates are produced. A **provider** is a
named strategy implementation for a semantic value, including custom scalars,
objects, and arrays. A **case** pairs a fixture with an expected consumer outcome.
An **oracle** is a caller-owned function that determines whether a fixture
reproduces a failure.

Fixtures may be request bodies for the API under test, responses returned by an
external dependency, or values passed directly to application code. Riff MUST
leave HTTP execution, request interception, recording, assertions, and test
scheduling to the host test tools. It MUST NOT contact endpoints during fixture
generation or execute source code embedded in a contract.

## 2. Fuzzing and property-testing principles

Implementations MUST separate generation from the property being checked.
They MUST validate pass fixtures against the schema, bound rejected draws, and
report exhaustion rather than return invalid pass data. They SHOULD explore
small values, boundary values, optional-field omission, nullable values, collection
sizes, and string edge cases alongside realistic values.

The strategy vocabulary is deliberately familiar:

| Concept | Riff mechanism |
| --- | --- |
| Generate from a contract | A schema strategy |
| Constants and explicit examples | Schema `const` / `enum`, examples, values strategy |
| Combine alternatives | A one-of strategy |
| Reject unsuitable candidates | A filter strategy or enclosing schema validation |
| Generate dependent fields | A provider returning the containing object |
| Mutate a valid example | A declared failure replacement at one JSON Pointer |
| Check a property | Caller-owned assertions or oracle |
| Replay a failure | Saved JSON payload; seeded replay within a pinned environment |
| Shrink a failure | Budgeted minimization using the caller's oracle |

Choice does not imply exhaustive branch coverage. Filters are rejection sampling,
not a solver. A successful sample does not establish a property for all inputs.
A generation failure does not prove that a schema is unsatisfiable.

Automatic coverage guidance, a persistent example database, automatic test
scheduling, arbitrary executable predicates in schemas, and global-minimum
shrinking are outside this draft. Mapping and dependent generation belong in
registered providers rather than language-specific expressions in the schema.

## 3. Inputs and schema selection

An implementation MUST accept a standalone schema in its advertised supported
subset. It SHOULD accept OpenAPI 3.0 and 3.1 documents and select either an
operation's request-body schema or an exact response status and media type.
Status selection MUST be explicit; a missing exact response MUST NOT silently
select a different status. Path templates are used as written in the document.

Local acyclic JSON Pointer references MUST resolve against the supplied document.
External references and cycles MUST fail without fetching remote content.
A reference with sibling keywords is unsupported in this draft; use `allOf`.
Pointer tokens use RFC 6901 escaping. Annotation data MUST NOT be traversed as
schema structure.

OpenAPI 3.0 `nullable: true` adds null to an explicit type. It does not override an
`enum` or other incompatible constraints. Boolean exclusive bounds in 3.0 are
translated to numeric exclusive bounds. Other dialects use their native null type
and numeric exclusive bounds.

Implementations MUST document supported keywords, formats, regex semantics,
resource limits, and dialect exclusions. They MUST reject unsupported validation
keywords rather than silently weaken them. Read/write annotations, defaults,
examples outside `x-fixture`, descriptions, and discriminators are informational
in this draft; they do not automatically add, remove, or correlate fields.

## 4. Portable fixture metadata

`x-fixture` MAY annotate any schema node:

```yaml
x-fixture:
  provider: email
  locale: en-US
  options: {}
  examples: ['known@example.test']
  fail:
    examples: ['malformed', null]
    reason: The endpoint must reject malformed email values.
```

The positive source may contain `provider`, `locale`, `options`, and `examples`,
or one `strategy`. A strategy MUST NOT coexist with those shorthand keys.
`examples` is a nonempty array of JSON values. Positive shorthand examples MUST
satisfy the annotated schema at compilation. Strategy values are candidates and
are validated at draw time. Unknown metadata keys MUST fail compilation.

`fail` contains a source with the same fields and an optional string `reason`.
It MUST declare a provider, examples, or a strategy. Failure examples MAY violate
the schema. The outer positive source and `fail` are independent.

Providers are named data, not paths to code. A document MUST NOT trigger dynamic
imports. Locale identifiers use hyphenated language-region notation such as
`en-US`; implementations MUST document supported locales. Options are JSON
objects interpreted by the selected provider.

## 5. Strategies

Each strategy is an object with a `kind` discriminator:

```yaml
kind: schema
```

Generate using the enclosing field schema, without reapplying that field's own
`x-fixture` annotation. Descendant field annotations still apply.

```yaml
kind: values
values: [0, 1, 100]
```

Choose from a nonempty explicit array. The enclosing schema still validates the
chosen value for a pass fixture.

```yaml
kind: provider
name: acme.account-id
locale: en-US
options: {prefix: acct}
```

Invoke a registered provider. Built-in names and custom names use the same
contract. Unknown names MUST fail before sampling.

```yaml
kind: one-of
strategies:
  - {kind: provider, name: email}
  - {kind: values, values: ['explicit@example.test']}
```

Choose one child strategy and draw from it. This differs from JSON Schema
`oneOf`, which requires exactly one schema branch to validate. Strategy choice
SHOULD give each child a chance; exact probabilities are implementation-specific.

```yaml
kind: filter
schema: {type: integer, minimum: 10}
strategy:
  kind: values
  values: [-1, 10, 11]
```

Draw from the child and reject values that fail the predicate schema. The
predicate uses the supported schema subset and local references scoped to that
predicate. It MUST NOT run executable code. Rejections MUST consume a finite
budget. Prefer direct constraints when they can express the desired generation.
Explicit strategies take precedence over generation-profile preferences.

## 6. Provider contract

A provider receives a fresh context containing:

- `seed`: a lossless unsigned 64-bit per-invocation seed;
- `locale`: the requested locale;
- `schema`: the normalized field schema;
- `options`: that invocation's provider options.

It returns exactly one JSON value. Scalars, arrays, and objects are permitted.
A provider SHOULD return the same value for identical inputs, use only its seed
for randomness, avoid wall-clock-dependent values, and avoid external state.
Context objects MUST be isolated from the compiled contract. Calling a provider
MUST NOT give it mutable access to another invocation's context.

Custom registrations MAY override built-in names. Built-in semantic names are
`email`, `full-name`, `company-name`, `url`, `phone-number`, `city`, `country-code`,
and `uuid`. Their exact text is not standardized. Defaults are string providers;
custom providers may produce any JSON type satisfying the enclosing schema.

All returned data MUST be JSON-compatible with finite numbers. Exceptions and
non-JSON output are provider failures. Schema-incompatible pass output is a
rejected draw. Failure providers may deliberately return incompatible data.

## 7. Profiles and exploration

`mixed` combines realistic/provider generation with structural exploration.
`realistic` prefers declared providers and retains explicit examples.
`boundary` emphasizes small and endpoint values, omissions, nullable alternatives,
and a string edge corpus. Schema-required formats still apply.

Profiles are sampling preferences, not validity modes or coverage guarantees.
An explicit strategy overrides the profile at that node. Native enum and const
constraints remain authoritative through validation. If a contract requires a
particular business state, express it as an explicit strategy or constraint.

Implementations SHOULD include empty and whitespace strings, composed and
decomposed Unicode, non-Latin text, emoji, control characters, and punctuation
when compatible with the schema. They MUST respect advertised resource limits.

## 8. Expected success and failure

A pass case MUST contain a schema-valid value. It declares that the caller
expects the consumer operation to succeed; Riff cannot establish business success.

A fail case MUST begin with a valid base fixture and replace one reachable field
using that field's explicit `fail` declaration. The root is a possible field.
Optional fields may be added when their parent exists. Array-item declarations
are reachable only in nonempty arrays. A replacement MUST change the JSON value.
No reachable declaration or only unchanged replacements is a generation failure.

Failure sources MAY preserve schema validity: an inactive account can be a
well-formed record that must be rejected by an operation. Implementations MUST
keep declared outcome separate from observed schema validity.

A case contains `value`, `expectation` (`pass` or `fail`), `schema_valid`, and
`violations`. Fail cases additionally identify `target` as a JSON Pointer and MAY
include `reason`. A violation contains its JSON Pointer and a diagnostic message.
The root pointer is the empty string. Diagnostics need not match across backends.
Type systems SHOULD distinguish validated successful values from raw, possibly
malformed failure values. Runtime schema constraints MUST still be checked.

## 9. Budgets, reproducibility, and replay

Every draw and rejection loop MUST have a finite budget. Implementations MUST
also bound nested generation and structural sizes, and document those bounds.
Budget exhaustion MUST fail the operation without returning a partial batch.
Budgets bound candidate work, not the running time of arbitrary provider code,
oracles, or regular expressions.

A supplied seed MUST reproduce a batch for identical schema, options, provider
behavior, implementation, runtime, and dependency versions. Separate batches MUST
NOT share mutable generator state. Unseeded calls MAY use nondeterministic seeds.
This specification does not mandate a cross-language random stream.

Saved fixture payloads are the authoritative replay artifact across upgrades.
Save the case envelope as well when labels, mutation locations, or reasons matter.
A seed alone is insufficient for replay across implementation versions.

## 10. Failure minimization

A minimizer takes a known failing value, a caller-supplied failure oracle, and an
oracle-evaluation budget. It MUST first confirm that the original value fails.
It SHOULD try smaller collections, omitted fields, shorter strings, and numbers
toward zero. A reduction is accepted only while the oracle still reports failure.

Schema preservation MUST be explicit and enabled by default. Malformed-input
failures require disabling it. Results MUST include the retained reproducer, the
number of oracle evaluations, and whether that budget was exhausted. A greedy
result is not guaranteed globally minimal. Oracle exceptions MUST propagate;
callers own test execution and cleanup. The oracle must be repeatable for useful
results. Minimization MUST NOT mutate the caller's original value.
