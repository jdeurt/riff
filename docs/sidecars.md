# Sidecar fixture files

Keep a vendor's OpenAPI document unchanged and put generation metadata in a
separate YAML file. [The example vendor schema](../examples/vendor.yaml) has no
inline fixture annotations; its [sidecar](../examples/vendor.fixtures.yaml)
supplies them.

```yaml
version: '0.1'
operations:
  - path: /customers/{id}
    method: get
    body: response
    status: '200'
    fields:
      /properties/name:
        provider: full-name
        locale: en-US
        examples: ['', '  Ada  ', '李明']
        fail:
          examples: [null, 42]
          reason: A non-string name must be rejected.
      /properties/email:
        provider: email
```

```python
factory = FixtureFactory.from_file(
    "../examples/vendor.yaml",
    path="/customers/{id}",
    fixture_file="../examples/vendor.fixtures.yaml",
)
```

```sh
uv run riff ../examples/vendor.yaml --path '/customers/{id}' \
  --fixtures ../examples/vendor.fixtures.yaml --count 10 --seed 42
```

## Endpoint selection

`operations` entries select an exact path template, HTTP method, body kind, and
media type. Defaults are `get`, `response`, status `'200'`, and `application/json`.
`mediaType` changes the media type. `status` matters only for responses. For your
own endpoint, use `body: request` and its method:

```yaml
version: '0.1'
operations:
  - path: /registrations
    method: post
    body: request
    fields:
      /properties/seats:
        fail:
          strategy: {kind: schema, constraints: violate}
```

The host API/CLI must select that same operation and body kind. Quote numeric
response statuses in YAML. If `operations` is present and nonempty, exactly one
entry must match the selection. Zero or multiple matches raise `SchemaError`.

## Schema paths

Field keys are plain RFC 6901 JSON Pointers into the **resolved body schema**.
They are schema paths, not paths into the generated payload:

| Meaning | Schema pointer |
| --- | --- |
| Root schema | `''` |
| A property named email | `/properties/email` |
| Every item's schema in a tags array | `/properties/tags/items` |
| A property inside the first allOf branch | `/allOf/0/properties/name` |
| A property literally named a/b~c | `/properties/a~1b~0c` |

References are expanded before attachment. A sidecar targets that occurrence of
the field, without modifying other uses of a shared component. Slash and tilde
characters in names use `~1` and `~0`. Percent signs are literal in these plain
pointers. Wildcards, JSONPath expressions, and paths into annotation/example data
are not supported. Missing paths and boolean-schema targets are errors.

## Standalone schemas and precedence

Use a top-level `fields` map for a standalone schema or shared annotations:

```yaml
version: '0.1'
fields:
  /properties/seats:
    strategy: {kind: values, values: [1, 25, 50]}
    fail:
      strategy: {kind: schema, constraints: violate}
```

`FixtureFactory.from_schema(schema, fixture_file="fixtures.yaml")` also accepts
this form. Operation-specific entries override top-level entries at the same
pointer. Each selected entry replaces the complete inline `x-fixture` annotation
at that field; it does not merge individual source keys. Inline metadata remains
supported. Custom providers are registered in code exactly as for inline hints.
