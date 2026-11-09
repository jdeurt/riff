# Shared Rust engine

Riff uses one engine for its Python and JavaScript packages. The portable
[specification](spec.md) defines the data contract; the Rust implementation in
[`rust/`](../rust/) interprets it.

| Layer | Responsibility |
| --- | --- |
| Rust core | Offline reference resolution, dialect selection, sidecars, metadata compilation, generation, filtering, mutation, validity diagnostics, minimization, publication filtering |
| PyO3 binding | Native handles, typed JSON transport, synchronous provider/oracle callbacks, Python exceptions |
| Python facade | Pydantic views, safe local file loading, Faker defaults, typed public API |
| Neon binding | Node-API handles, typed JSON transport, synchronous callbacks, JavaScript exceptions |
| JavaScript facade | Standard Schema views, safe local file loading, Faker defaults, TypeScript declarations |

The facades do not implement competing generators. Providers supply semantic
values through an isolated callback. Each binding uses its language's Faker
package by default and accepts custom providers through the same public
contract. Rust callers supply a provider callback to `Factory`.

Each batch owns its random stream, provider state, and budgets. A compiled factory
holds the contract and validators. Python threads can reuse a factory without
sharing mutable generation state. JavaScript calls and callbacks are synchronous;
asynchronous model validators, providers, and oracles are unsupported.

Callback seeds travel through the private native ABI as decimal strings. Public
providers receive Python `int`, JavaScript `bigint`, or Rust `u64`. These preserve
all 64 bits. This wire detail does not change the portable provider contract.

Native JSON is decoded without silently rounding numbers. A binding rejects an
unsupported numeric representation instead of returning an altered fixture or
misleading validity flag. See [numeric limits](json-schema.md#numbers-and-limits).

## Build and verify

Install Rust through [rustup](https://rustup.rs/); `rust-toolchain.toml` pins the
toolchain and installs rustfmt and Clippy. Python requires 3.10+ and uv.
JavaScript requires Node 22.12+ and npm.

From the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p riff-core
```

From `python/`:

```sh
uv sync --locked
uv run ruff check .
uv run ruff format --check .
uv run ty check
uv run pytest tests examples
uv build
```

After editing Rust, rebuild the Python extension before running Python checks:

```sh
uv sync --locked --reinstall-package riff
```

From `javascript/`:

```sh
npm ci
npm run check
npm pack
```

`npm run build` compiles the Neon addon, emits JavaScript and declarations, and
copies the platform library into `native/index.node`. The npm tarball includes
that binary and is specific to the platform where it was built. Python wheels
use PyO3's Python 3.10 stable ABI. Building either package from source requires
Rust and a platform C linker. Registry publishing and prebuilt release binaries
are separate release operations.

The [tooling policy](tooling.md) explains enforced checks. CI runs Rust checks,
Python 3.10–3.14, Node 22/24, package tests, and a shared replay workflow.

## Rust API

The core's public structs and operations have rustdoc. Generate local API docs:

```sh
cargo doc --locked -p riff-core --no-deps
```

`prepare` selects and normalizes a schema. `Factory::new` validates metadata and
registered provider names. `Factory::generate`, `failure_cases`, and `minimize`
perform complete operations with caller-supplied callbacks. No schema value can
cause an endpoint request, dynamic import, or remote reference fetch.
