from __future__ import annotations

from pathlib import Path
from typing import Any

import pytest

import uuideal


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--cpython-uuid-ref",
        default=None,
        help="CPython git ref used for pytest-collected test.test_uuid cases.",
    )
    parser.addoption(
        "--cpython-uuid-tests-dir",
        default=None,
        help="Directory containing or receiving the sparse CPython checkout used for test.test_uuid.",
    )
    parser.addoption(
        "--cpython-uuid-unpatched",
        action="store_true",
        help="Run pytest-collected CPython uuid cases without installing uuideal.",
    )


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers",
        "cpython_uuid: CPython Lib/test/test_uuid.py compatibility cases",
    )
    config.addinivalue_line(
        "markers",
        "performance: timing-sensitive smoke tests that should run only in stable environments",
    )


@pytest.fixture(autouse=True)
def clean_patch_state():
    uuideal.uninstall()
    yield
    uuideal.uninstall()


ORDERED_TEST_FILES = [
    "test_cpython_uuid.py",
    ...,
    "test_uuid_fork.py",
    "test_uuid_multiprocessing.py",
    "test_uuid_fuzz.py",
]

ORDERED_MARKERS = [
    ...,
    "performance",
]


def position(value: str | ..., ordered_values: list[str | ...]) -> int:
    try:
        return ordered_values.index(value)

    except ValueError:
        return ordered_values.index(...)


def sort_key(item: Any) -> tuple[int, int, str]:
    path = Path(str(item.fspath))

    marker = "performance" if item.get_closest_marker("performance") is not None else None

    test_file = path.name if path.name in ORDERED_TEST_FILES else None

    return (
        position(marker, ORDERED_MARKERS),
        position(test_file, ORDERED_TEST_FILES),
        item.nodeid,
    )


def pytest_collection_modifyitems(items: list[Any]) -> None:
    items.sort(key=sort_key)

    cpython_uuid_prefix = "tests/test_cpython_uuid.py::test_cpython_uuid["
    cpython_uuid_suffix = "]"

    for item in items:
        nodeid = item.nodeid
        if not nodeid.startswith(cpython_uuid_prefix) or not nodeid.endswith(cpython_uuid_suffix):
            continue

        unittest_id = nodeid.removeprefix(cpython_uuid_prefix).removesuffix(cpython_uuid_suffix)
        item._nodeid = f"tests/test_cpython_uuid.py::{unittest_id}"
        item.name = unittest_id
