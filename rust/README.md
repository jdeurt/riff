# Riff for Rust

Schema-guided JSON fixtures with semantic providers, declared failure cases,
seeded replay, and minimization. See the [portable spec](../docs/spec.md) for
fixture behavior and [schema guide](../docs/json-schema.md) for supported contracts.

Generate API documentation from the repository root:

```sh
cargo doc --locked -p riff-core --no-deps
```

Run the required checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p riff-core
```

Follow [AGENTS.md](../AGENTS.md) when contributing Rust changes.

The [input-fuzzing suite](../docs/input-fuzzing.md) uses the full JSON Schema,
OpenAPI, and sidecar contracts in `schemas/`:

```sh
cargo test --locked -p riff-core --test input_fuzz
```
