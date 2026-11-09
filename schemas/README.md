# Schemas used for dogfooding

`run-options.schema.json` describes the Python binding's
validated generation invocation options. It is exported from `GenerationOptions`.
`examples/riff-options.fixtures.yaml` adds small test workloads and failure cases
without changing that contract.

`bounded-integer-schema.schema.json` describes a finite, satisfiable grammar of
integer schemas. Riff generates schemas from it, then generates bounded values
from those schemas. It is intentionally smaller than the JSON Schema metaschema.

See [the dogfooding guide](../docs/dogfooding.md) for the executed workflows and
independently checked outcomes.
