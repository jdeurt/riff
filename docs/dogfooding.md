# Riff tests Riff

The Python suite uses Riff as a fixture source for its own public workflows.
[The tests](../python/tests/test_dogfood.py) exercise three independent paths.

1. **Generate run options, then generate fixtures.** Riff reads its
   [run-options schema](../schemas/run-options.schema.json) with a
   [bounded sampling sidecar](../examples/riff-options.fixtures.yaml). The results
   decode into typed `GenerationOptions`. Each set of options drives a real
   generation call; tests check cardinality, integer bounds, and seeded replay.
2. **Generate schemas, then use them.** A
   [bounded integer-schema grammar](../schemas/bounded-integer-schema.schema.json)
   produces constrained schemas. Riff compiles each and generates values. Tests
   check each returned integer directly against that generated schema's bounds.
3. **Generate invalid options, then call the CLI.** Failure declarations produce
   bad counts, seeds, profile names, or attempt limits. Tests invoke the real CLI
   and require exit code 2, a diagnostic, and no partial payload or traceback.

The sidecar restricts test workloads to small batches; the public options schema
still describes the full accepted option range. Assertions check application
behavior, rather than merely asking Riff to validate its own output.

From `python/`:

```sh
uv run pytest tests/test_dogfood.py
```

This is bounded bootstrapping. It does not claim that Riff generates every legal
JSON Schema or supports the recursive official JSON Schema metaschema. The
ordinary suite also has independently constructed contracts, typed endpoint
examples, CLI tests, provider isolation checks, and explicit failure regressions.
