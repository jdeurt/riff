"""Attach sidecar metadata to resolved schema locations selected by operation."""

from __future__ import annotations

from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING, Literal
from urllib.parse import quote

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError

from riff.compiler import JSON_ADAPTER, NODE_ADAPTER, FixtureMetadata, Node
from riff.errors import SchemaError
from riff.schema import resolve_pointer

if TYPE_CHECKING:
    from riff.types import Schema


class OperationSelector(BaseModel):
    """Select an exact OpenAPI request or response body."""

    model_config = ConfigDict(extra="forbid", strict=True)
    path: str
    method: str = "get"
    body: Literal["request", "response"] = "response"
    status: str = "200"
    media_type: str = Field(default="application/json", alias="mediaType")


class OperationOverlay(OperationSelector):
    """Attach field metadata to one selected operation body."""

    fields: dict[str, FixtureMetadata]

    def matches(self, selector: OperationSelector) -> bool:
        """Compare all selectors that participate in the selected body kind."""
        return (
            self.path == selector.path
            and self.method.lower() == selector.method.lower()
            and self.body == selector.body
            and self.media_type == selector.media_type
            and (self.body == "request" or self.status == selector.status)
        )


class OverlayDocument(BaseModel):
    """Versioned portable sidecar data, with global and operation-specific fields."""

    model_config = ConfigDict(extra="forbid", strict=True)
    version: Literal["0.1"]
    fields: dict[str, FixtureMetadata] = Field(default_factory=dict)
    operations: list[OperationOverlay] = Field(default_factory=list)


def apply_fixture_file(
    schema: Schema, source: str | Path, *, selector: OperationSelector | None
) -> Schema:
    """Read a sidecar, select its annotations, and replace complete field metadata."""
    try:
        with Path(source).open(encoding="utf-8") as stream:
            document = OverlayDocument.model_validate(yaml.safe_load(stream))
    except (OSError, UnicodeError, yaml.YAMLError, ValidationError) as error:
        message = f"Unable to read fixture sidecar {source}: {error}"
        raise SchemaError(message) from error
    fields = dict(document.fields)
    if document.operations:
        if selector is None:
            message = (
                "Operation-specific fixture annotations require an OpenAPI operation selection."
            )
            raise SchemaError(message)
        matches = [operation for operation in document.operations if operation.matches(selector)]
        if len(matches) != 1:
            message = f"Fixture sidecar must match exactly one operation; found {len(matches)}."
            raise SchemaError(message)
        fields.update(matches[0].fields)
    try:
        parsed = NODE_ADAPTER.validate_python(schema, strict=True)
    except ValidationError as error:
        message = f"Invalid schema metadata before sidecar attachment: {error}"
        raise SchemaError(message) from error
    locations = _schema_paths(parsed, "")
    result = deepcopy(schema)
    for pointer, metadata in fields.items():
        if pointer not in locations:
            message = f"Fixture schema path does not select a resolved schema object: {pointer!r}"
            raise SchemaError(message)
        target = resolve_pointer(result, f"#{quote(pointer, safe='/~')}")
        if not isinstance(target, dict):
            message = f"Fixture schema path cannot annotate a boolean schema: {pointer!r}"
            raise SchemaError(message)
        target["x-fixture"] = JSON_ADAPTER.validate_python(
            metadata.model_dump(mode="json", by_alias=True, exclude_unset=True)
        )
    return result


def _schema_paths(node: Node, path: str) -> set[str]:
    if isinstance(node, bool):
        return set()
    paths = {path}
    for keyword, children in (
        ("properties", node.properties),
        ("patternProperties", node.patterns),
    ):
        for name, child in children.items():
            escaped = name.replace("~", "~0").replace("/", "~1")
            paths.update(_schema_paths(child, f"{path}/{keyword}/{escaped}"))
    for keyword, branches in (
        ("allOf", node.all_of),
        ("anyOf", node.any_of),
        ("oneOf", node.one_of),
    ):
        for index, child in enumerate(branches):
            paths.update(_schema_paths(child, f"{path}/{keyword}/{index}"))
    for keyword, child in (
        ("items", node.items),
        ("additionalProperties", node.additional),
        ("not", node.negated),
        ("contains", node.contains),
    ):
        if child is not None:
            paths.update(_schema_paths(child, f"{path}/{keyword}"))
    return paths
