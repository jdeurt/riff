"""Read local contracts without silently rounding numeric constraints."""

from __future__ import annotations

import json
from decimal import Decimal
from math import isfinite
from pathlib import Path
from typing import TYPE_CHECKING

import yaml
from typing_extensions import override

from riff.compiler import JSON_ADAPTER
from riff.errors import GenerationError, SchemaError

if TYPE_CHECKING:
    from yaml.nodes import ScalarNode

    from riff.types import JSONValue


class ContractLoader(yaml.SafeLoader):
    """Reject YAML floats whose original decimal value would be changed."""

    @override
    def construct_yaml_float(self, node: ScalarNode) -> float:
        """Preserve the decimal meaning or reject this external numeric representation."""
        try:
            return _parse_json_float(node.value.replace("_", ""))
        except ValueError as error:
            message = "Numeric YAML value cannot be preserved by Python."
            raise SchemaError(message) from error


ContractLoader.add_constructor("tag:yaml.org,2002:float", ContractLoader.construct_yaml_float)


def read_document(path: str | Path) -> JSONValue:
    """Read finite, representable local JSON/YAML; no reference fetching occurs."""
    try:
        content = Path(path).read_text(encoding="utf-8")
        data: object
        if Path(path).suffix.lower() == ".json":
            data = json.loads(content, parse_float=_parse_json_float)
        else:
            loader = ContractLoader(content)
            try:
                data = loader.get_single_data()
            finally:
                loader.dispose()
        return JSON_ADAPTER.validate_python(data)
    except (OSError, UnicodeError, yaml.YAMLError, ValueError) as error:
        message = f"Unable to read contract {path}: {error}"
        raise SchemaError(message) from error


def _parse_json_float(token: str) -> float:
    value = float(token)
    if not isfinite(value) or Decimal(str(value)) != Decimal(token):
        message = "JSON contains a number Python cannot preserve."
        raise ValueError(message)
    return value


def decode_native(payload: str) -> JSONValue:
    """Decode native numbers losslessly, rejecting unsupported float projections."""
    try:
        decoded: object = json.loads(payload, parse_float=_parse_json_float)
        return JSON_ADAPTER.validate_python(decoded)
    except ValueError as error:
        raise GenerationError(str(error)) from error
