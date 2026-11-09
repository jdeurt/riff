# Strategies, filtering, and minimization

A schema defines acceptable values. A strategy defines candidate generation.
The enclosing schema remains the validation oracle for successful draws.

## Choose between sources

```yaml
type: string
format: email
x-fixture:
  strategy:
    kind: one-of
    strategies:
      - kind: provider
        name: email
      - kind: values
        values: ['explicit@example.test']
      - kind: schema
```

`schema` uses structural generation for that node while keeping descendant
annotations. `provider` invokes a semantic provider. `values` chooses from an
explicit corpus. `one-of` chooses a child strategy; it does not require exactly
one JSON Schema branch to match. Every listed child can be selected, but no
coverage quota is promised.

An explicit strategy applies in every profile. Provider shorthand is a preference:
`realistic` uses it, `mixed` combines it with structural generation, and `boundary`
explores structural edge cases. If a test requires a particular state, express
that state as a strategy rather than relying on a preference.

## Filter candidates

```yaml
type: integer
x-fixture:
  strategy:
    kind: filter
    schema: {minimum: 10}
    strategy:
      kind: values
      values: [-1, 10, 11]
```

The filter rejects `-1`. Predicate schemas are portable data, not executable
callbacks. Prefer direct constraints or a targeted provider when a filter would
reject nearly every draw. `max_attempts` bounds rejection loops; a shared nested
candidate budget also prevents composed filters from retrying indefinitely.
Impossible or overly restrictive searches raise `GenerationError`.

## Explicit examples and negative mutations

Use `examples` to add known valid edge cases to a provider/structural source.
They are checked during compilation. Use `fail.examples` for deliberate negative
cases. A failure case starts with a valid body and replaces exactly one declared
field. Its JSON Pointer appears in `case.target`; its declared expectation and
observed schema validity remain separate.

Keep property assertions in your test framework. A label explains what the test
expects; it does not prove the application handled the case correctly.

## Reduce a failing value

```python
result = factory.minimize(
    failing_body,
    is_failure=reproduces_bug,
    max_evaluations=100,
)
save_regression_fixture(result.value)
```

`reproduces_bug` accepts a JSON value and returns a boolean. It must first return
true for the original value. The minimizer tries smaller values and retains only
reductions that still fail. Schema preservation defaults to true. Use
`preserve_schema=False` to minimize deliberately malformed payloads.

`result` reports `value`, `evaluations`, and `budget_exhausted`. Reduction is
greedy, not a proof of global minimality. The oracle owns test execution, cleanup,
and side effects; its exceptions propagate. Save the reduced JSON as a regression
fixture. Riff does not create an example database or automatically rerun tests.
