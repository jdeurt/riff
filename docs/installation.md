# Installation and development

Python requires 3.10+ and [uv](https://docs.astral.sh/uv/). JavaScript requires
Node 22.12+ and npm. Source builds also require [Rust](https://rustup.rs/) and a
platform C linker. The repository pins the required Rust toolchain.

## Python

From `python/`:

```sh
uv sync --locked
uv run pytest tests examples
uv build
```

Install from Git in another project:

```sh
uv add 'riff @ git+https://github.com/jdeurt/riff.git#subdirectory=python'
```

Use `uv sync --locked --reinstall-package riff` when rebuilding after source changes.
See the [quick start](quickstart.md) for fixture generation and endpoint tests.

## JavaScript

From `javascript/`:

```sh
npm ci
npm run check
npm pack
```

Install the resulting tarball into your project:

```sh
npm install /path/to/riff/javascript/riff-fixtures-0.1.0.tgz
```

Build packages on their target operating system and architecture. These commands
create local artifacts; neither package has been published to a registry.
The [JavaScript guide](javascript-api.md) covers typed models and examples.

## Checks

The [tooling policy](tooling.md) lists strict lint, formatting, and type checks.
CI tests Python 3.10–3.14, Node 22/24, package builds, endpoint workflows, and
[Riff’s three input contracts](input-fuzzing.md). Self-fuzzing runs in the Rust
package; language-specific tests cover the public interfaces.
