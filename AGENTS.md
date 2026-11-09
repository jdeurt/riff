# Rust development

For Rust changes in `rust/` and `bindings/`, follow Microsoft's
[Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/agents/all.txt).
Read the applicable guidelines before editing and use them during review.
Explicit user requirements and this repository's stricter lint rules take precedence.

Keep shared behavior in the Rust core. PyO3 and Neon translate host values,
callbacks, and errors; Python and JavaScript expose typed interfaces and model
integration. Do not duplicate generation, parsing, validation, or CLI policy in
language packages. In particular, follow `M-FFI-TRANSLATES`.

Use the pinned Rust toolchain. Do not weaken workspace lints or add unchecked
casts, unsafe source, panic-based input handling, or undocumented public APIs.
Check changes from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p riff-core
```

Exercise behavior through public workflows. Rebuild both native extensions before
testing their facades, and verify shared behavior with the cross-binding replay.
