# Riff core

The shared Rust engine implements Riff's portable schema, strategy, provider,
case, and minimization behavior. PyO3 and Neon expose it through the Python and
JavaScript packages. Host adapters supply Faker defaults and custom callbacks.

From the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p riff-core
cargo doc --locked -p riff-core --no-deps
```

Public types and methods have rustdoc. See [architecture](../docs/architecture.md)
for ownership and build details, [the spec](../docs/spec.md) for portable behavior,
and [tooling](../docs/tooling.md) for enforced lint rules.
