# Riff for JavaScript

Generate JSON fixtures from JSON Schema or OpenAPI for Node 22.12+ tests. Use
built-in or custom providers, YAML sidecars, typed model views, labeled
success/failure cases, seeded replay, and minimization.

For a source build, install Rust and run from this directory:

```sh
npm ci
npm run check
npm pack
```

Install the resulting tarball into your Node project. Build packages on the
target operating system and architecture; registry publishing is a separate step.

Start with the [JavaScript guide and API](../docs/javascript-api.md). The
[endpoint tests](tests/endpoints.test.ts) show typed request fixtures and real
HTTP vendor responses. See [installation](../docs/installation.md),
[schema constraints](../docs/json-schema.md), and the
[portable specification](../docs/spec.md) for compatibility and limits.
