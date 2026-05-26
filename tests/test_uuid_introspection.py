from __future__ import annotations

import inspect
import uuid
from typing import Any

import uuideal


def callable_metadata(candidate: Any) -> dict[str, Any]:
    return {
        "__name__": getattr(candidate, "__name__", None),
        "__qualname__": getattr(candidate, "__qualname__", None),
        "__module__": getattr(candidate, "__module__", None),
        "__doc__": getattr(candidate, "__doc__", None),
        "__annotations__": getattr(candidate, "__annotations__", None),
        "__defaults__": getattr(candidate, "__defaults__", None),
        "__kwdefaults__": getattr(candidate, "__kwdefaults__", None),
        "__text_signature__": getattr(candidate, "__text_signature__", None),
        "signature": str(inspect.signature(candidate)),
        "doc": inspect.getdoc(candidate),
    }


def uuid_introspection_targets() -> list[Any]:
    targets: list[Any] = [
        uuid.UUID,
        uuid.UUID.__init__,
        uuid.UUID.__str__,
        uuid.UUID.__repr__,
        uuid.UUID.__hash__,
        uuid.uuid1,
        uuid.uuid3,
        uuid.uuid4,
        uuid.uuid5,
    ]
    for name in ("uuid6", "uuid7", "uuid8"):
        if hasattr(uuid, name):
            targets.append(getattr(uuid, name))
    if hasattr(uuid.UUID, "_from_int"):
        targets.append(uuid.UUID._from_int)
    return targets


def test_introspection_metadata_matches_stdlib_after_install() -> None:
    targets = uuid_introspection_targets()
    expected = {target: callable_metadata(target) for target in targets}

    uuideal.install()

    actual = {target: callable_metadata(target) for target in targets}

    assert actual == expected


def test_bound_method_introspection_still_works_after_install() -> None:
    value = uuid.UUID("12345678-1234-5678-9234-567812345678")
    expected = {
        "__str__": str(inspect.signature(value.__str__)),
        "__repr__": str(inspect.signature(value.__repr__)),
        "__hash__": str(inspect.signature(value.__hash__)),
        "__int__": str(inspect.signature(value.__int__)),
    }

    uuideal.install()

    actual_value = uuid.UUID("12345678-1234-5678-9234-567812345678")
    actual = {
        "__str__": str(inspect.signature(actual_value.__str__)),
        "__repr__": str(inspect.signature(actual_value.__repr__)),
        "__hash__": str(inspect.signature(actual_value.__hash__)),
        "__int__": str(inspect.signature(actual_value.__int__)),
    }

    assert actual == expected