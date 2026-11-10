# Rust development

For Rust changes in `rust/` and `bindings/`, follow Microsoft's
[Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/agents/all.txt).
Read the applicable guidelines before editing and use them during review.
Explicit user requirements and this repository's stricter lint rules take precedence.

Keep shared behavior in the Rust core. PyO3 and Neon translate host values,
callbacks, and errors; Python and JavaScript expose typed interfaces and model
integration. Do not duplicate generation, parsing, validation, or CLI policy in
language packages. In particular, follow `M-FFI-TRANSLATES`.

Language package runtime code is limited to exports, native loading, typed model
adapters, and CLI stream handling. Expose factory operations and metadata helpers
directly from the bindings. Keep request assembly, JSON conversion, provider
dispatch, result presentation, and native error translation in the bindings.

Use the pinned Rust toolchain. Do not weaken workspace lints or add unchecked
casts, unsafe source, panic-based input handling, or undocumented public APIs.
Check changes from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p riff-core
```

Exercise behavior through public workflows. Rebuild both native extensions before
testing their interfaces. Keep self-fuzzing and its three input schemas in `rust/`.

Self-fuzzing has exactly three input contracts: full JSON Schema, full OpenAPI,
and full sidecar schemas in `rust/schemas/`. Keep their tests in Rust. Do not add
language-specific dogfooding schemas or top-level conformance fixtures.

Generate self-fuzzing batches directly through the Rust factory from those full
contracts. Do not add sampling schemas, hand-built input generators, or fixture
annotations/sidecars to steer JSON Schema or OpenAPI dogfooding.
