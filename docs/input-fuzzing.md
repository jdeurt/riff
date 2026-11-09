# Fuzzing Riff's inputs

Riff's self-fuzzing suite exercises exactly three input contracts:

- [JSON Schema](../rust/schemas/json-schema.json): complete official Draft 7,
  2019-09, and 2020-12 metaschemas, selected by the declared dialect.
- [OpenAPI](../rust/schemas/openapi.json): complete official OpenAPI 3.0 and 3.1
  document schemas. Selected body schemas are also checked as JSON Schema.
- [Sidecars](../rust/schemas/sidecar.json): the complete annotation-file format,
  including operation selectors, schema pointers, positive/failure sources,
  provider settings, and recursive strategy combinations.

Referenced schema resources are included for offline checks. Sampling profiles
bound candidate sizes and recursion; they do not replace the full input contracts.
Riff generates the candidate inputs, which must satisfy the full contracts before
being used in the tested operations.

The suite checks emitted values against known bounds and selected bodies, verifies
sidecar overrides and failure targets, and requires known-invalid mutations to be
rejected. It also checks boolean schemas and unsatisfiable inputs. Runs use fixed
seeds and finite budgets; they do not establish exhaustive coverage.

From the repository root:

```sh
cargo test --locked -p riff-core --test input_fuzz
```

The [tests](../rust/tests/input_fuzz.rs) and their sampling profiles live with the
Rust package. Python and JavaScript tests cover their public interfaces, typed
models, callbacks, and endpoint examples.
