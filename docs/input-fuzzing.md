# Fuzzing Riff's inputs

Riff generates 1,024 documents directly from each of three complete input schemas:

- [JSON Schema](../rust/schemas/json-schema.json): the official Draft 7,
  2019-09, and 2020-12 metaschemas, selected by the declared dialect.
- [OpenAPI](../rust/schemas/openapi.json): the official OpenAPI 3.0 and 3.1
  document schemas.
- [Sidecars](../rust/schemas/sidecar.json): the complete sidecar format,
  including selectors, schema pointers, provider settings, and strategies.

Each test loads the full schema, creates a factory, and generates a batch:

```rust
let factory = Factory::new(&schema)?;
let values = factory.generate(&GenerationOptions {
    count: 1024,
    seed: Some(42),
    ..GenerationOptions::default()
})?;
```

JSON Schema and OpenAPI generation use no fixture annotations or sidecars.
Referenced resources are included in the supplied schemas. An independent
validator checks every generated document against its complete input schema;
the tests also require varied results.

Metaschema-valid schemas can still be unsatisfiable or refer to resources absent
from the generated document. This suite tests document generation, with fixed
seeds and finite budgets. It does not establish exhaustive coverage.

From the repository root:

```sh
cargo test --locked -p riff-core --test input_fuzz
```

The [tests](../rust/tests/input_fuzz.rs) live with the Rust package. Separate
workflow tests cover schema application, selectors, overrides, and declared
failures. Python and JavaScript tests cover their public interfaces, typed models,
callbacks, and endpoint examples.
