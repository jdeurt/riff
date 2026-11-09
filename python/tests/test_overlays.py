from pathlib import Path

import pytest
from pydantic import BaseModel

from riff import FailureCase, FixtureFactory, SchemaError

ROOT = Path(__file__).resolve().parents[2]


class NamedCustomer(BaseModel):
    id: str
    name: str
    status: str
    balance_cents: int
    email: str | None = None
    tags: list[str] = []


def test_sidecar_attaches_to_expanded_response_fields_and_generates_failures() -> None:
    factory = FixtureFactory.from_file(
        ROOT / "examples/vendor.yaml",
        path="/customers/{id}",
        fixture_file=ROOT / "examples/vendor.fixtures.yaml",
    ).as_model(NamedCustomer)
    values = factory.generate(count=30, seed=42, profile="realistic")
    assert any(value.name == "李明" for value in values)
    assert any(" " in value.name for value in values)
    failures = factory.failure_cases(count=10, seed=42)
    assert all(
        isinstance(case, FailureCase) and case.target == "/name" and not case.schema_valid
        for case in failures
    )


def test_operation_sidecar_replaces_inline_hints_and_selects_request_body(tmp_path: Path) -> None:
    sidecar = tmp_path / "fixtures.yaml"
    sidecar.write_text(
        """version: '0.1'
operations:
  - path: /registrations
    method: POST
    body: request
    fields:
      /properties/name:
        strategy: {kind: values, values: [Sidecar]}
""",
        encoding="utf-8",
    )
    factory = FixtureFactory.from_file(
        ROOT / "examples/own-api.yaml",
        path="/registrations",
        method="post",
        body="request",
        fixture_file=sidecar,
    )
    for value in factory.generate(count=10, seed=1):
        assert isinstance(value, dict)
        assert value["name"] == "Sidecar"


def test_sidecar_paths_support_compositions_and_escaped_property_names(tmp_path: Path) -> None:
    sidecar = tmp_path / "fixtures.yaml"
    sidecar.write_text(
        """version: '0.1'
fields:
  /allOf/0/properties/a~1b~0c:
    strategy: {kind: values, values: [attached]}
""",
        encoding="utf-8",
    )
    factory = FixtureFactory.from_schema(
        {
            "allOf": [
                {
                    "type": "object",
                    "required": ["a/b~c"],
                    "properties": {"a/b~c": {"type": "string"}},
                }
            ]
        },
        fixture_file=sidecar,
    )
    for value in factory.generate(count=5, seed=9):
        assert isinstance(value, dict)
        assert value["a/b~c"] == "attached"


@pytest.mark.parametrize(
    "fields",
    [
        "/properties/missing",
        "/properties/count/default",
        "/properties/count~2bad",
    ],
)
def test_bad_sidecar_paths_are_not_silently_ignored(tmp_path: Path, fields: str) -> None:
    sidecar = tmp_path / "fixtures.yaml"
    sidecar.write_text(
        f"version: '0.1'\nfields:\n  {fields}: {{examples: [1]}}\n", encoding="utf-8"
    )
    with pytest.raises(SchemaError, match="schema path"):
        FixtureFactory.from_schema(
            {"type": "object", "properties": {"count": {"type": "integer", "default": {}}}},
            fixture_file=sidecar,
        )


@pytest.mark.parametrize("operation", ["/missing", "/customers/{id}"])
def test_missing_or_ambiguous_operation_selections_are_errors(
    tmp_path: Path, operation: str
) -> None:
    sidecar = tmp_path / "fixtures.yaml"
    declaration = f"  - path: {operation}\n    fields: {{}}\n"
    sidecar.write_text("version: '0.1'\noperations:\n" + declaration * 2, encoding="utf-8")
    with pytest.raises(SchemaError, match="exactly one operation"):
        FixtureFactory.from_file(
            ROOT / "examples/vendor.yaml", path="/customers/{id}", fixture_file=sidecar
        )
