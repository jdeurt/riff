# Providers

A provider describes the meaning of a generated value. The schema constrains its
shape; the provider supplies domain knowledge. Built-in names are `email`,
`full-name`, `company-name`, `url`, `phone-number`, `city`,
`country-code`, and `uuid`.

```yaml
type: string
x-fixture:
  provider: full-name
  locale: en-US
```

Locale names use language-region spelling. Built-ins support `en-US`, `en-GB`,
`de-DE`, `fr-FR`, and `ja-JP` in Python and JavaScript. Unsupported built-in
locales fail at compilation. Custom providers decide which locales and options
they support.

The same Riff version, contract, options, and seed produce the same built-in data
in Python and JavaScript for values supported by both languages. Save fixture
payloads for replay across upgrades; exact generated text can change by version.

## Register a custom semantic type

```python
from riff import FixtureFactory, JSONValue, ProviderContext

def account_id(context: ProviderContext) -> JSONValue:
    return f"acct_{context.seed:016x}"

factory = FixtureFactory.from_schema(
    {
        "type": "string",
        "pattern": "^acct_[0-9a-f]{16}$",
        "x-fixture": {"provider": "acme.account-id"},
    },
    providers={"acme.account-id": account_id},
)
fixtures = factory.generate(count=5, seed=7, profile="realistic")
```

The public contract is `ProviderContext -> JSONValue`. Context contains a
per-invocation seed, locale, normalized schema, and provider options. Identical
contexts should yield identical values. Use local random state; avoid global
seeding and the wall clock. Riff snapshots context JSON so provider mutations
cannot change the compiled contract or another call's inputs.

Providers may return an entire object or array. This is the idiomatic way to
produce related fields such as matching country/postcode values, an amount and
its currency, or a start/end date pair. Register a domain-specific name on the
containing object. The same output validation applies to whole-object providers.

A registration can replace a default name. Schema files never import code;
registration happens in the host language. The CLI deliberately supports only
built-in providers. Use the library for custom registrations.

## Failure providers

```yaml
type: integer
minimum: 0
x-fixture:
  provider: acme.balance
  fail:
    provider: acme.overdrawn-balance
    options: {minimum_debt: 100}
    reason: Overdrawn accounts must be rejected.
```

Register both names. Failure sources can also use explicit examples or complete
strategy trees. A failure may be schema-invalid or schema-valid; the resulting
case reports which one occurred.

Unregistered names raise `SchemaError` before sampling. Ordinary exceptions and
non-JSON returns raise `ProviderError`, retaining the original exception as the
cause. Python cancellation and process-exit exceptions propagate unchanged.
Schema-incompatible successful output consumes the
generation budget and eventually raises `GenerationError`. Provider execution
itself has no timeout; providers should do bounded local work.

JavaScript uses the same contract with `ProviderContext.seed` represented as
`bigint`; Python uses `int`. See the
[JavaScript provider example](javascript-api.md#custom-providers).
