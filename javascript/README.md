# Riff for JavaScript

Typed Node API over Riff's shared Rust engine, connected through Neon. Supports
Node 22.12+, portable JSON Schema/OpenAPI fixtures, YAML sidecars, Faker defaults,
custom providers, success/failure cases, and minimization.

Install Rust through [rustup](https://rustup.rs/), then run from this directory:

```sh
npm ci
npm run check
npm pack
```

The build emits JavaScript and documented TypeScript declarations, compiles the
native addon, and includes it in the platform-specific npm tarball. Install that
tarball into another Node project; registry publishing is a separate release step.

Start with the [JavaScript guide and API](../docs/javascript-api.md). The
[endpoint tests](tests/endpoints.test.ts) show typed request fixtures and real
HTTP vendor responses. The [shared specification](../docs/spec.md),
[schema guide](../docs/json-schema.md), [architecture](../docs/architecture.md),
and [tooling policy](../docs/tooling.md) describe compatibility and limits.
