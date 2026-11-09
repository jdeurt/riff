# Riff for Python

Python 3.12+ reference implementation of the [Riff specification](../docs/spec.md).

```sh
uv sync --locked
uv run riff ../examples/vendor.yaml --path '/customers/{id}' --count 10 --seed 42
uv run pytest
uv run pytest examples/test_endpoint.py
uv run python -m examples.consumer
```

Run these commands from `python/`. Development dependencies include the tools
and libraries used by the examples; production dependencies are declared
separately. The workspace lockfile is at the repository root.

See the [quick start](../docs/quickstart.md), [API reference](../docs/python-api.md),
[provider guide](../docs/providers.md), [strategy guide](../docs/strategies.md),
and [endpoint testing guide](../docs/testing.md).
