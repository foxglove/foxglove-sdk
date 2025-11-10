import json
from dataclasses import asdict, dataclass
from enum import Enum
from typing import Any


def _snake_to_camel(snake_str: str) -> str:
    """Convert snake_case string to camelCase."""
    components = snake_str.split("_")
    return components[0] + "".join(word.capitalize() for word in components[1:])


def _process_value(value: Any) -> Any:
    """Recursively process values: convert dicts, filter None, handle lists, handle enums."""
    if value is None:
        return None
    if isinstance(value, dict):
        return {
            _snake_to_camel(k): _process_value(v)
            for k, v in value.items()
            if v is not None
        }
    if isinstance(value, list):
        return [_process_value(item) for item in value if item is not None]
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, DataclassModel):
        return value.to_dict()
    return value


@dataclass
class DataclassModel:
    def to_dict(self) -> dict[str, Any]:
        """
        - Iterate over all props and recursively convert to dict
        - Convert snake_case to camelCase
        - Filter out None values
        - Return the dict
        """
        data = asdict(self)
        return {
            _snake_to_camel(k): _process_value(v)
            for k, v in data.items()
            if v is not None
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)
