from __future__ import annotations

import gc
import sys
import time
import uuid
import weakref
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

import pytest

import uuideal


@dataclass(frozen=True)
class RecordedOutcome:
    kind: str
    value: Any = None
    error_type: type[BaseException] | None = None
    error_message: str | None = None


def record_outcome(call: Callable[[], Any]) -> RecordedOutcome:
    try:
        return RecordedOutcome("value", call())
    except BaseException as error:
        return RecordedOutcome("error", error_type=type(error), error_message=str(error))


def assert_same_outcome(actual: RecordedOutcome, expected: RecordedOutcome) -> None:
    assert actual.kind == expected.kind
    if expected.kind == "error":
        assert actual.error_type is expected.error_type
        assert actual.error_message == expected.error_message
    else:
        assert actual.value == expected.value


class MyStr(str):
    pass


@pytest.mark.parametrize(
    "text",
    [
        MyStr("12345678123456789234567812345678"),
        MyStr("12345678-1234-5678-9234-567812345678"),
        " 12345678123456789234567812345678",
        "12345678123456789234567812345678 ",
        "\t12345678123456789234567812345678",
        "12345678-1234-5678-9234-567812345678\r\n",
        "\n12345678-1234-5678-9234-567812345678",
        "12345678-1234-5678-9234-567812345678\t",
    ],
)
def test_textual_parser_edge_cases_match_stdlib(text: str) -> None:
    expected = record_outcome(lambda: uuid.UUID(text))

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(text))
    assert_same_outcome(actual, expected)


def test_hex_keyword_accepts_or_rejects_str_subclass_like_stdlib() -> None:
    text = MyStr("12345678123456789234567812345678")
    expected = record_outcome(lambda: uuid.UUID(hex=text))

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(hex=text))
    assert_same_outcome(actual, expected)


class IntLike:
    def __int__(self) -> int:
        return 1


class IndexLike:
    def __index__(self) -> int:
        return 1


class BytesLike:
    def __bytes__(self) -> bytes:
        return b"\x00" * 16


@pytest.mark.parametrize(
    ("keyword", "value"),
    [
        ("int", True),
        ("int", IntLike()),
        ("int", IndexLike()),
        ("bytes", bytearray(b"\x00" * 16)),
        ("bytes", memoryview(b"\x00" * 16)),
        ("bytes", BytesLike()),
        ("bytes_le", bytearray(b"\x00" * 16)),
        ("bytes_le", memoryview(b"\x00" * 16)),
        ("bytes_le", BytesLike()),
        ("fields", [0, 0, 0, 0, 0, 0]),
        ("fields", (True, True, True, True, True, True)),
    ],
)
def test_constructor_argument_type_edges_match_stdlib(keyword: str, value: object) -> None:
    expected = record_outcome(lambda: uuid.UUID(**{keyword: value}))

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(**{keyword: value}))
    assert_same_outcome(actual, expected)


def test_weakref_callback_behavior_matches_stdlib() -> None:
    expected_called: list[weakref.ReferenceType[uuid.UUID]] = []

    def expected_callback(reference: weakref.ReferenceType[uuid.UUID]) -> None:
        expected_called.append(reference)

    expected_reference = weakref.ref(uuid.UUID(int=1), expected_callback)
    assert expected_reference() is None
    gc.collect()
    expected_callback_count = len(expected_called)

    uuideal.install()

    actual_called: list[weakref.ReferenceType[uuid.UUID]] = []

    def actual_callback(reference: weakref.ReferenceType[uuid.UUID]) -> None:
        actual_called.append(reference)

    actual_reference = weakref.ref(uuid.UUID(int=1), actual_callback)
    assert actual_reference() is None
    gc.collect()
    actual_callback_count = len(actual_called)

    assert actual_callback_count == expected_callback_count


def test_uuid_object_layout_invariants_match_stdlib() -> None:
    value = uuid.UUID(int=1)
    expected = {
        "has_dict": hasattr(value, "__dict__"),
        "dir": sorted(dir(value)),
        "basicsize": getattr(uuid.UUID, "__basicsize__", None),
        "itemsize": getattr(uuid.UUID, "__itemsize__", None),
        "sizeof": sys.getsizeof(value),
        "slots": getattr(uuid.UUID, "__slots__", None),
    }

    uuideal.install()

    actual_value = uuid.UUID(int=1)
    actual = {
        "has_dict": hasattr(actual_value, "__dict__"),
        "dir": sorted(dir(actual_value)),
        "basicsize": getattr(uuid.UUID, "__basicsize__", None),
        "itemsize": getattr(uuid.UUID, "__itemsize__", None),
        "sizeof": sys.getsizeof(actual_value),
        "slots": getattr(uuid.UUID, "__slots__", None),
    }

    assert actual == expected


def test_uuid1_explicit_node_and_clock_seq_values_are_unique_under_load() -> None:
    uuideal.install()

    values = [uuid.uuid1(node=0x102030405060, clock_seq=1) for _ in range(1000)]

    assert len({value.int for value in values}) == len(values)
    assert all(type(value) is uuid.UUID for value in values)
    assert all(value.version == 1 for value in values)
    assert all(value.variant == uuid.RFC_4122 for value in values)
    assert all(value.node == 0x102030405060 for value in values)
    assert all(value.clock_seq == 1 for value in values)


def test_uuid7_timestamp_bits_are_close_to_unix_epoch_milliseconds() -> None:
    before_ms = int(time.time() * 1000) - 1000
    value = uuideal.uuid7()
    after_ms = int(time.time() * 1000) + 1000

    timestamp_ms = value.int >> 80

    assert type(value) is uuid.UUID
    assert value.version == 7
    assert value.variant == uuid.RFC_4122
    assert before_ms <= timestamp_ms <= after_ms


@pytest.mark.performance
def test_uuid7_many_values_are_unique_and_sorted() -> None:
    values = [uuideal.uuid7() for _ in range(100_000)]

    assert values == sorted(values)
    assert len({value.int for value in values}) == len(values)
    assert all(type(value) is uuid.UUID for value in values)
    assert all(value.version == 7 for value in values)
    assert all(value.variant == uuid.RFC_4122 for value in values)