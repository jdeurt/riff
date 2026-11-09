# Riff for Python

Python 3.12+ reference implementation of the [Riff specification](../docs/spec.md).

```sh
uv sync --locked
uv run riff ../examples/vendor.yaml --path '/customers/{id}' \
  --fixtures ../examples/vendor.fixtures.yaml --count 10 --seed 42
uv run pytest tests examples
uv run python -m examples.consumer
```

Run these commands from `python/`. Development dependencies include the tools
and libraries used by the examples; production dependencies are declared
separately. The workspace lockfile is at the repository root.

Start with the [quick start](../docs/quickstart.md). Then choose
[typed metadata](../docs/model-metadata.md) or [sidecar annotations](../docs/sidecars.md),
and follow the [endpoint testing guide](../docs/testing.md).
The [API reference](../docs/python-api.md) covers signatures, errors, and limits;
the [root guide index](../README.md#guides) links the remaining topics.
