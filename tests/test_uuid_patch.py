from __future__ import annotations

import copy
import gc
import operator
import pickle
import random
import re
import sys
import threading
import timeit
import uuid
import weakref
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

import pytest

import uuideal


UUID_ATTRIBUTE = (
    "int",
    "bytes",
    "bytes_le",
    "hex",
    "fields",
    "time_low",
    "time_mid",
    "time_hi_version",
    "clock_seq_hi_variant",
    "clock_seq_low",
    "node",
    "time",
    "clock_seq",
    "urn",
    "variant",
    "version",
    "is_safe",
)

UUID_SPECIAL_CALLABLE_ATTRIBUTES = (
    "__repr__",
    "__str__",
    "__hash__",
    "__int__",
    "__dir__",
)


@dataclass(frozen=True)
class MissingAttribute:
    name: str


@dataclass
class Snapshot:
    instance: uuid.UUID
    instance_attributes: dict[str, Any]
    class_attributes: dict[str, Any]
    special_values: dict[str, Any]
    size: int


@dataclass(frozen=True)
class RecordedOutcome:
    kind: str
    value: Any = None
    error_type: type[BaseException] | None = None
    error_message: str | None = None

    def describe(self) -> str:
        if self.kind == "error":
            error_name = self.error_type.__name__ if self.error_type is not None else None
            return f"error(type={error_name!r}, message={self.error_message!r})"
        return f"value({self.value!r})"


def record_outcome(call: Callable[[], Any]) -> RecordedOutcome:
    try:
        return RecordedOutcome("value", call())
    except BaseException as error:
        return RecordedOutcome("error", error_type=type(error), error_message=str(error))


def assert_same_outcome(
    actual: RecordedOutcome, expected: RecordedOutcome, context: str = ""
) -> None:
    assert actual.kind == expected.kind, (
        f"{context}\nexpected: {expected.describe()}\nactual:   {actual.describe()}"
    )
    if expected.kind == "error":
        assert actual.error_type is expected.error_type, (
            f"{context}\nexpected: {expected.describe()}\nactual:   {actual.describe()}"
        )
        assert actual.error_message == expected.error_message, (
            f"{context}\nexpected: {expected.describe()}\nactual:   {actual.describe()}"
        )
    else:
        assert actual.value == expected.value, (
            f"{context}\nexpected: {expected.describe()}\nactual:   {actual.describe()}"
        )


def uuid_observation(value: uuid.UUID) -> dict[str, Any]:
    return {
        "type": type(value),
        "snapshot": snapshot(value),
        "str": str(value),
        "repr": repr(value),
        "hash": hash(value),
        "int": int(value),
        "sizeof": sys.getsizeof(value),
        "copy": copy.copy(value),
        "deepcopy": copy.deepcopy(value),
        "pickle": {
            protocol: pickle.dumps(value, protocol=protocol)
            for protocol in range(pickle.HIGHEST_PROTOCOL + 1)
        },
        "pickle_loads": {
            protocol: pickle.loads(pickle.dumps(value, protocol=protocol))
            for protocol in range(pickle.HIGHEST_PROTOCOL + 1)
        },
    }


def get_class_attribute_or_missing(candidate_cls: type[uuid.UUID], attribute: str) -> Any:
    try:
        return object.__getattribute__(candidate_cls, attribute)
    except AttributeError:
        return MissingAttribute(attribute)


def snapshot(candidate: uuid.UUID) -> Snapshot:
    instance_attributes = {
        attribute: object.__getattribute__(candidate, attribute) for attribute in UUID_ATTRIBUTE
    }
    candidate_cls = type(candidate)
    class_attributes = {
        attribute: get_class_attribute_or_missing(candidate_cls, attribute)
        for attribute in (*UUID_ATTRIBUTE, *UUID_SPECIAL_CALLABLE_ATTRIBUTES)
    }
    special_values = {name: getattr(candidate, name)() for name in UUID_SPECIAL_CALLABLE_ATTRIBUTES}

    return Snapshot(
        candidate, instance_attributes, class_attributes, special_values, sys.getsizeof(candidate)
    )


def test_install_preserves_function_identities() -> None:
    uuid4 = uuid.uuid4
    uuid1 = uuid.uuid1
    uuid_class_init = uuid.UUID.__init__
    uuid_class_str = uuid.UUID.__str__
    uuid6 = getattr(uuid, "uuid6", None)
    uuid7 = getattr(uuid, "uuid7", None)

    uuideal.install()
    uuideal.install()

    assert uuid.uuid4 is uuid4
    assert uuid.uuid1 is uuid1
    if uuid6 is not None:
        assert uuid.uuid6 is uuid6
    if uuid7 is not None:
        assert uuid.uuid7 is uuid7
    assert uuid.UUID.__init__ is uuid_class_init
    assert uuid.UUID.__str__ is uuid_class_str
    assert uuideal.installed()

    uuideal.uninstall()
    uuideal.uninstall()

    assert uuid.uuid4 is uuid4
    assert uuid.uuid1 is uuid1
    if uuid6 is not None:
        assert uuid.uuid6 is uuid6
    if uuid7 is not None:
        assert uuid.uuid7 is uuid7
    assert uuid.UUID.__init__ is uuid_class_init
    assert uuid.UUID.__str__ is uuid_class_str
    assert not uuideal.installed()


def test_preinstall_aliases_observe_patched_behavior() -> None:
    imported_UUID = uuid.UUID
    imported_uuid1 = uuid.uuid1
    imported_uuid3 = uuid.uuid3
    imported_uuid4 = uuid.uuid4
    imported_uuid5 = uuid.uuid5
    imported_uuid6 = getattr(uuid, "uuid6", None)
    imported_uuid7 = getattr(uuid, "uuid7", None)
    imported_uuid8 = getattr(uuid, "uuid8", None)

    uuideal.install()

    constructed = imported_UUID("12345678-1234-5678-9234-567812345678")
    assert type(constructed) is uuid.UUID
    assert constructed.hex == "12345678123456789234567812345678"

    uuid1_value = imported_uuid1(node=0x123456789ABC, clock_seq=0x1234)
    assert type(uuid1_value) is uuid.UUID
    assert uuid1_value.version == 1
    assert uuid1_value.variant == uuid.RFC_4122
    assert uuid1_value.node == 0x123456789ABC
    assert uuid1_value.clock_seq == 0x1234

    uuid3_value = imported_uuid3(uuid.NAMESPACE_DNS, "python.org")
    assert type(uuid3_value) is uuid.UUID
    assert uuid3_value == uuid.UUID("6fa459ea-ee8a-3ca4-894e-db77e160355e")

    uuid4_value = imported_uuid4()
    assert type(uuid4_value) is uuid.UUID
    assert uuid4_value.version == 4
    assert uuid4_value.variant == uuid.RFC_4122

    uuid5_value = imported_uuid5(uuid.NAMESPACE_DNS, "python.org")
    assert type(uuid5_value) is uuid.UUID
    assert uuid5_value == uuid.UUID("886313e1-3b8a-5372-9b90-0c9aee199e5d")

    if imported_uuid6 is not None:
        uuid6_value = imported_uuid6(node=0x123456789ABC, clock_seq=0x1234)
        assert type(uuid6_value) is uuid.UUID
        assert uuid6_value.version == 6
        assert uuid6_value.variant == uuid.RFC_4122
        assert uuid6_value.node == 0x123456789ABC
        assert uuid6_value.clock_seq == 0x1234

    if imported_uuid7 is not None:
        uuid7_value = imported_uuid7()
        assert type(uuid7_value) is uuid.UUID
        assert uuid7_value.version == 7
        assert uuid7_value.variant == uuid.RFC_4122

    if imported_uuid8 is not None:
        uuid8_value = imported_uuid8(0x123456789ABC, 0xABC, 0x123456789ABCDEF)
        assert type(uuid8_value) is uuid.UUID
        assert uuid8_value.version == 8
        assert uuid8_value.variant == uuid.RFC_4122


@pytest.mark.parametrize(
    ("constructor_kwargs", "constructor_args"),
    [
        ({}, ("{12345678-1234-5678-9234-567812345678}",)),
        ({}, ("12345678123456789234567812345678",)),
        ({}, ("urn:uuid:12345678-1234-5678-9234-567812345678",)),
        ({"hex": "12345678-1234-5678-9234-567812345678"}, ()),
        ({"bytes": bytes.fromhex("12345678123456789234567812345678")}, ()),
        ({"bytes_le": bytes.fromhex("78563412341278569234567812345678")}, ()),
        ({"int": 0x12345678123456789234567812345678}, ()),
        ({"fields": (0x12345678, 0x1234, 0x5678, 0x92, 0x34, 0x567812345678)}, ()),
        ({"hex": "12345678123456781234567812345678", "version": 4}, ()),
    ],
)
def test_uuid_constructor_paths_match_stdlib(constructor_kwargs, constructor_args) -> None:
    expected = uuid.UUID(*constructor_args, **constructor_kwargs)
    expected_snapshot = snapshot(expected)
    uuideal.install()
    actual = uuid.UUID(*constructor_args, **constructor_kwargs)
    assert snapshot(actual) == expected_snapshot


def test_patched_properties_work_on_unpatched_objects() -> None:
    stdlib_object = uuid.UUID("12345678-1234-5678-9234-567812345678")
    expected_values = {
        name: getattr(stdlib_object, name)
        for name in (
            "bytes",
            "bytes_le",
            "hex",
            "fields",
            "time_low",
            "time_mid",
            "time_hi_version",
            "clock_seq_hi_variant",
            "clock_seq_low",
            "node",
            "time",
            "clock_seq",
            "urn",
            "variant",
            "version",
        )
    }

    uuideal.install()

    for name, expected in expected_values.items():
        assert getattr(stdlib_object, name) == expected, name
    assert str(stdlib_object) == "12345678-1234-5678-9234-567812345678"
    assert repr(stdlib_object) == "UUID('12345678-1234-5678-9234-567812345678')"


@pytest.mark.parametrize("factory", [uuid.uuid3, uuid.uuid5])
@pytest.mark.parametrize(
    "namespace",
    [uuid.NAMESPACE_DNS, uuid.NAMESPACE_URL, uuid.NAMESPACE_OID, uuid.NAMESPACE_X500],
)
@pytest.mark.parametrize("name", ["", "python.org", "тест", b"bytes-name"])
def test_deterministic_factories_match_stdlib(factory, namespace, name) -> None:
    expected = factory(namespace, name)
    expected_snapshot = snapshot(expected)
    uuideal.install()
    actual = factory(namespace, name)
    assert snapshot(actual) == expected_snapshot
    assert actual.variant == uuid.RFC_4122


def test_uuid4_returns_stdlib_uuid_with_version_and_uniqueness() -> None:
    uuideal.install()
    values = [uuid.uuid4() for _ in range(100)]
    assert all(type(value) is uuid.UUID for value in values)
    assert all(value.version == 4 for value in values)
    assert all(value.variant == uuid.RFC_4122 for value in values)
    assert len({value.int for value in values}) == len(values)


def test_uuid1_explicit_inputs_have_expected_static_fields() -> None:
    uuideal.install()
    value = uuid.uuid1(node=0x123456789ABC, clock_seq=0x1234)
    assert type(value) is uuid.UUID
    assert value.version == 1
    assert value.variant == uuid.RFC_4122
    assert value.node == 0x123456789ABC
    assert value.clock_seq == 0x1234
    assert value.is_safe == uuid.SafeUUID.unknown


def test_uuid6_shortcut_and_patch_have_expected_static_fields() -> None:
    shortcut_value = uuideal.uuid6(node=0x123456789ABC, clock_seq=0x1234)
    assert type(shortcut_value) is uuid.UUID
    assert shortcut_value.version == 6
    assert shortcut_value.variant == uuid.RFC_4122
    assert shortcut_value.node == 0x123456789ABC
    assert shortcut_value.clock_seq == 0x1234
    assert shortcut_value.is_safe == uuid.SafeUUID.unknown

    if hasattr(uuid, "uuid6"):
        uuid6_identity = uuid.uuid6
        uuideal.install()
        patched_value = uuid.uuid6(node=0x123456789ABC, clock_seq=0x1234)
        assert uuid.uuid6 is uuid6_identity
        assert type(patched_value) is uuid.UUID
        assert patched_value.version == 6
        assert patched_value.variant == uuid.RFC_4122
        assert patched_value.node == 0x123456789ABC
        assert patched_value.clock_seq == 0x1234


def test_uuid7_shortcut_and_patch_have_expected_fields() -> None:
    shortcut_values = [uuideal.uuid7() for _ in range(100)]
    assert all(type(value) is uuid.UUID for value in shortcut_values)
    assert all(value.version == 7 for value in shortcut_values)
    assert all(value.variant == uuid.RFC_4122 for value in shortcut_values)
    assert len({value.int for value in shortcut_values}) == len(shortcut_values)
    assert shortcut_values == sorted(shortcut_values)

    if hasattr(uuid, "uuid7"):
        uuid7_identity = uuid.uuid7
        uuideal.install()
        patched_values = [uuid.uuid7() for _ in range(100)]
        assert uuid.uuid7 is uuid7_identity
        assert all(type(value) is uuid.UUID for value in patched_values)
        assert all(value.version == 7 for value in patched_values)
        assert all(value.variant == uuid.RFC_4122 for value in patched_values)
        assert len({value.int for value in patched_values}) == len(patched_values)
        assert patched_values == sorted(patched_values)


@pytest.mark.parametrize("version", [1, 2, 3, 4, 5, 6, 7, 8])
@pytest.mark.parametrize("raw", [0, 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF])
def test_constructor_version_forces_bits_exactly(raw: int, version: int) -> None:
    expected = uuid.UUID(int=raw, version=version)

    uuideal.install()

    actual = uuid.UUID(int=raw, version=version)
    assert actual == expected
    assert actual.version == version
    assert actual.variant == uuid.RFC_4122
    assert actual.int == expected.int


@pytest.mark.parametrize(
    ("value_int", "expected_variant"),
    [
        (0x00000000000000000000000000000000, uuid.RESERVED_NCS),
        (0x00000000000000008000000000000000, uuid.RFC_4122),
        (0x0000000000000000C000000000000000, uuid.RESERVED_MICROSOFT),
        (0x0000000000000000E000000000000000, uuid.RESERVED_FUTURE),
    ],
)
def test_variant_boundaries_match_stdlib(value_int: int, expected_variant: str) -> None:
    expected = uuid.UUID(int=value_int)

    uuideal.install()

    actual = uuid.UUID(int=value_int)
    assert actual.variant == expected.variant == expected_variant
    assert actual.version == expected.version


@pytest.mark.parametrize(
    "call",
    [
        pytest.param(lambda: uuid.uuid4(node=1), id="uuid.uuid4(node=1)"),
        pytest.param(lambda: uuid.uuid4(clock_seq=1), id="uuid.uuid4(clock_seq=1)"),
        pytest.param(lambda: uuid.uuid1(1, 2, 3), id="uuid.uuid1(1, 2, 3)"),
        pytest.param(
            lambda: uuid.uuid3(namespace=uuid.NAMESPACE_DNS, name="x"),
            id='uuid.uuid3(namespace=NAMESPACE_DNS, name="x")',
        ),
        pytest.param(
            lambda: uuid.uuid5(namespace=uuid.NAMESPACE_DNS, name="x"),
            id='uuid.uuid5(namespace=NAMESPACE_DNS, name="x")',
        ),
        pytest.param(
            lambda: uuid.UUID(hex="0" * 32, unknown=1), id='uuid.UUID(hex="0" * 32, unknown=1)'
        ),
        pytest.param(
            lambda: uuid.UUID("0" * 32, version=4, is_safe=uuid.SafeUUID.safe),
            id='uuid.UUID("0" * 32, version=4, is_safe=safe)',
        ),
    ],
)
def test_call_protocol_outcomes_match_stdlib(call: Callable[[], object]) -> None:
    expected = record_outcome(call)

    uuideal.install()

    actual = record_outcome(call)
    assert_same_outcome(actual, expected)


def test_constructor_rejects_multiple_sources_pairwise() -> None:
    sources: dict[str, Any] = {
        "hex": "12345678123456781234567812345678",
        "bytes": bytes.fromhex("12345678123456789234567812345678"),
        "bytes_le": bytes.fromhex("78563412341278569234567812345678"),
        "fields": (0x12345678, 0x1234, 0x5678, 0x92, 0x34, 0x567812345678),
        "int": 0x12345678123456789234567812345678,
    }
    items = list(sources.items())

    for left_index, (left_name, left_value) in enumerate(items):
        for right_name, right_value in items[left_index + 1 :]:
            kwargs = {left_name: left_value, right_name: right_value}
            expected = record_outcome(lambda kwargs=kwargs: uuid.UUID(**kwargs))
            assert expected.kind == "error"

            uuideal.install()
            actual = record_outcome(lambda kwargs=kwargs: uuid.UUID(**kwargs))
            uuideal.uninstall()

            assert_same_outcome(actual, expected, f"{left_name=} {right_name=}")


@pytest.mark.parametrize(
    "value",
    [
        uuid.UUID("12345678-1234-5678-9234-567812345678"),
        uuid.UUID("ffffffff-ffff-ffff-ffff-ffffffffffff"),
        uuid.UUID("00000000-0000-0000-0000-000000000000"),
    ],
)
def test_bytes_le_roundtrip(value: uuid.UUID) -> None:
    expected_bytes_le = value.bytes_le

    uuideal.install()

    actual = uuid.UUID(bytes_le=expected_bytes_le)
    assert actual == value
    assert actual.bytes_le == expected_bytes_le
    assert actual.bytes == value.bytes


@pytest.mark.parametrize("attribute", ["int", "hex", "bytes", "fields", "version"])
def test_uuid_attributes_remain_readonly(attribute: str) -> None:
    value = uuid.UUID(int=1)

    expected_set = record_outcome(lambda: setattr(value, attribute, 123))
    expected_del = record_outcome(lambda: delattr(value, attribute))

    uuideal.install()

    actual = uuid.UUID(int=1)
    actual_set = record_outcome(lambda: setattr(actual, attribute, 123))
    actual_del = record_outcome(lambda: delattr(actual, attribute))

    assert_same_outcome(actual_set, expected_set, f"set {attribute}")
    assert_same_outcome(actual_del, expected_del, f"del {attribute}")


@pytest.mark.parametrize("is_safe", [None, "safe", 1, object()])
def test_invalid_is_safe_matches_stdlib(is_safe: object) -> None:
    expected = record_outcome(lambda: uuid.UUID(int=1, is_safe=is_safe))

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(int=1, is_safe=is_safe))
    assert_same_outcome(actual, expected)


@pytest.mark.parametrize("operation", [operator.lt, operator.le, operator.gt, operator.ge])
def test_ordering_against_non_uuid_matches_stdlib(
    operation: Callable[[object, object], object],
) -> None:
    left = uuid.UUID(int=1)
    expected = record_outcome(lambda: operation(left, object()))

    uuideal.install()

    actual_left = uuid.UUID(int=1)
    actual = record_outcome(lambda: operation(actual_left, object()))

    assert_same_outcome(actual, expected)


def test_pickle_created_unpatched_loads_after_install_and_reverse() -> None:
    value = uuid.UUID("12345678-1234-5678-9234-567812345678")

    unpatched_payloads = [
        pickle.dumps(value, protocol=protocol) for protocol in range(pickle.HIGHEST_PROTOCOL + 1)
    ]

    uuideal.install()

    for payload in unpatched_payloads:
        loaded = pickle.loads(payload)
        assert type(loaded) is uuid.UUID
        assert loaded == value

    patched_payloads = [
        pickle.dumps(
            uuid.UUID("12345678-1234-5678-9234-567812345678"),
            protocol=protocol,
        )
        for protocol in range(pickle.HIGHEST_PROTOCOL + 1)
    ]

    uuideal.uninstall()

    for payload in patched_payloads:
        loaded = pickle.loads(payload)
        assert type(loaded) is uuid.UUID
        assert loaded == value


def test_uninstall_restores_stdlib_observable_behavior() -> None:
    expected = uuid_observation(uuid.UUID("12345678-1234-5678-9234-567812345678"))

    uuideal.install()
    _ = uuid.UUID("ffffffff-ffff-ffff-ffff-ffffffffffff").hex
    uuideal.uninstall()

    actual = uuid_observation(uuid.UUID("12345678-1234-5678-9234-567812345678"))
    assert actual == expected


def test_uuid4_does_not_use_python_random_getrandbits(monkeypatch: pytest.MonkeyPatch) -> None:
    def broken_getrandbits(bits: int) -> int:
        raise AssertionError(f"uuid4 must not use random.getrandbits({bits})")

    monkeypatch.setattr(random, "getrandbits", broken_getrandbits)

    uuideal.install()

    value = uuid.uuid4()
    assert type(value) is uuid.UUID
    assert value.version == 4
    assert value.variant == uuid.RFC_4122


@pytest.mark.parametrize(
    "text",
    [
        "１２３４５６７８１２３４５６７８１２３４５６７８１２３４５６７８",
        "12345678-1234-5678-9234-567812345678\n",
        "\x0012345678123456789234567812345678",
        "12345678_1234_5678_9234_567812345678",
        "urn:UUID:12345678-1234-5678-9234-567812345678",
        "URN:UUID:12345678-1234-5678-9234-567812345678",
    ],
)
def test_specific_weird_hex_inputs_match_stdlib(text: str) -> None:
    expected = record_outcome(lambda: uuid.UUID(text))

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(text))
    assert_same_outcome(actual, expected)


def test_patched_uuid_properties_work_on_subclass_instances() -> None:
    class MyUUID(uuid.UUID):
        pass

    expected = MyUUID("12345678-1234-5678-9234-567812345678")
    expected_snapshot = snapshot(expected)

    uuideal.install()

    actual = MyUUID("12345678-1234-5678-9234-567812345678")
    assert type(actual) is MyUUID
    assert snapshot(actual) == expected_snapshot


def weakref_observation() -> tuple[type[weakref.ReferenceType[uuid.UUID]], bool]:
    reference = weakref.ref(uuid.UUID(int=1))
    return type(reference), reference() is None


def test_weakref_behavior_matches_stdlib() -> None:
    expected = record_outcome(weakref_observation)

    uuideal.install()

    actual = record_outcome(weakref_observation)
    assert_same_outcome(actual, expected)


def test_gc_tracking_matches_stdlib() -> None:
    expected = gc.is_tracked(uuid.UUID(int=1))

    uuideal.install()

    actual = gc.is_tracked(uuid.UUID(int=1))
    assert actual is expected


def test_uuid_class_metadata_matches_stdlib() -> None:
    expected = {
        "__slots__": getattr(uuid.UUID, "__slots__", None),
        "__module__": uuid.UUID.__module__,
        "__doc__": uuid.UUID.__doc__,
        "__match_args__": getattr(uuid.UUID, "__match_args__", None),
    }

    uuideal.install()

    actual = {
        "__slots__": getattr(uuid.UUID, "__slots__", None),
        "__module__": uuid.UUID.__module__,
        "__doc__": uuid.UUID.__doc__,
        "__match_args__": getattr(uuid.UUID, "__match_args__", None),
    }

    assert actual == expected


def test_uuid_pattern_matching_behavior_matches_stdlib() -> None:
    value = uuid.UUID(int=1)

    def classify(candidate: object) -> str:
        match candidate:
            case uuid.UUID():
                return "uuid"
            case _:
                return "other"

    expected = classify(value)

    uuideal.install()

    actual = classify(uuid.UUID(int=1))
    assert actual == expected

@pytest.mark.performance
def test_concurrent_install_uninstall_while_reading_existing_objects() -> None:
    values = [uuid.UUID(int=index) for index in range(1000)]
    expected_hexes = [value.hex for value in values]
    stop = threading.Event()
    errors: list[BaseException] = []

    def toggler() -> None:
        try:
            for _ in range(10_000):
                uuideal.install()
                uuideal.uninstall()
        except BaseException as error:
            errors.append(error)
        finally:
            stop.set()

    def reader() -> None:
        try:
            while not stop.is_set():
                for value, expected_hex in zip(values, expected_hexes):
                    assert value.hex == expected_hex
                    assert uuid.UUID(hex=expected_hex) == value
        except BaseException as error:
            errors.append(error)
            stop.set()

    threads = [threading.Thread(target=toggler)]
    threads.extend(threading.Thread(target=reader) for _ in range(7))

    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()

    uuideal.uninstall()
    assert not errors


INVALID_UUID_CALLS = [
    pytest.param(lambda: uuid.UUID(), id="uuid.UUID()"),
    pytest.param(lambda: uuid.UUID("bad"), id='uuid.UUID("bad")'),
    pytest.param(lambda: uuid.UUID(bytes=b"short"), id='uuid.UUID(bytes=b"short")'),
    pytest.param(lambda: uuid.UUID(int=1 << 128), id="uuid.UUID(int=1 << 128)"),
    pytest.param(lambda: uuid.UUID(fields=(1, 2, 3)), id="uuid.UUID(fields=(1, 2, 3))"),
    pytest.param(
        lambda: uuid.UUID(
            "12345678123456781234567812345678",
            hex="12345678123456781234567812345678",
        ),
        id='uuid.UUID("12345678123456781234567812345678", hex="12345678123456781234567812345678")',
    ),
    pytest.param(lambda: uuid.uuid4(1), id="uuid.uuid4(1)"),
    pytest.param(lambda: uuid.uuid3(uuid.NAMESPACE_DNS), id="uuid.uuid3(uuid.NAMESPACE_DNS)"),
]


@pytest.mark.parametrize("call", INVALID_UUID_CALLS)
def test_error_equivalence_for_representative_invalid_inputs(
    call: Callable[[], object],
) -> None:
    with pytest.raises(Exception) as captured:
        call()

    expected_type = type(captured.value)
    expected_message = str(captured.value)

    uuideal.install()

    with pytest.raises(expected_type, match=re.escape(expected_message)):
        call()


def test_serialization_copy_and_subclassing() -> None:
    class MyUUID(uuid.UUID):
        def __init__(self, *args, custom_field=None, **kwargs):
            super().__init__(*args, **kwargs)
            self.custom_field = custom_field

    expected = uuid.UUID("12345678-1234-5678-9234-567812345678")
    uuideal.install()
    actual = uuid.UUID("12345678-1234-5678-9234-567812345678")

    for protocol in range(pickle.HIGHEST_PROTOCOL + 1):
        assert pickle.dumps(actual, protocol=protocol) == pickle.dumps(expected, protocol=protocol)
        assert pickle.loads(pickle.dumps(actual, protocol=protocol)) == expected

    assert copy.copy(actual) == expected
    assert copy.deepcopy(actual) == expected

    custom = MyUUID("12345678-1234-5678-9234-567812345678", custom_field="kept")
    assert type(custom) is MyUUID
    assert custom.custom_field == "kept"
    assert custom == expected


def test_from_int_matches_stdlib_and_preserves_subclasses() -> None:
    if not hasattr(uuid.UUID, "_from_int"):
        pytest.skip("uuid.UUID._from_int is not available on this Python version")

    class MyUUID(uuid.UUID):
        pass

    value = 0x12345678123456789234567812345678
    expected = uuid.UUID._from_int(value)
    expected_subclass = MyUUID._from_int(value)

    uuideal.install()

    actual = uuid.UUID._from_int(value)
    actual_subclass = MyUUID._from_int(value)

    assert actual == expected
    assert type(actual) is uuid.UUID
    assert actual_subclass == expected_subclass
    assert type(actual_subclass) is MyUUID
    assert uuid.UUID._from_int(True).int is True

    for invalid_value in (-1, 1 << 128):
        with pytest.raises(AssertionError, match=repr(invalid_value)):
            uuid.UUID._from_int(invalid_value)


def test_native_state_methods_match_stdlib() -> None:
    unknown = uuid.UUID(int=1)
    safe = uuid.UUID(int=1, is_safe=uuid.SafeUUID.safe)
    expected_unknown_state = unknown.__getstate__()
    expected_safe_state = safe.__getstate__()

    uuideal.install()

    actual_unknown = uuid.UUID(int=1)
    actual_safe = uuid.UUID(int=1, is_safe=uuid.SafeUUID.safe)
    assert actual_unknown.__getstate__() == expected_unknown_state
    assert actual_safe.__getstate__() == expected_safe_state

    restored_unknown = object.__new__(uuid.UUID)
    restored_unknown.__setstate__(expected_unknown_state)
    assert restored_unknown == unknown
    assert restored_unknown.is_safe is uuid.SafeUUID.unknown

    restored_safe = object.__new__(uuid.UUID)
    restored_safe.__setstate__(expected_safe_state)
    assert restored_safe == safe
    assert restored_safe.is_safe is uuid.SafeUUID.safe


def test_native_comparison_and_hash_methods_match_stdlib() -> None:
    left = uuid.UUID(int=1)
    same = uuid.UUID(int=1)
    right = uuid.UUID(int=2)
    operations = (
        operator.eq,
        operator.lt,
        operator.gt,
        operator.le,
        operator.ge,
    )
    expected = {
        operation: (operation(left, same), operation(left, right)) for operation in operations
    }
    expected_hash = hash(left)
    expected_not_implemented = uuid.UUID.__eq__(left, object())

    uuideal.install()

    actual_left = uuid.UUID(int=1)
    actual_same = uuid.UUID(int=1)
    actual_right = uuid.UUID(int=2)
    actual = {
        operation: (operation(actual_left, actual_same), operation(actual_left, actual_right))
        for operation in operations
    }
    assert actual == expected
    assert hash(actual_left) == expected_hash
    assert uuid.UUID.__eq__(actual_left, object()) is expected_not_implemented


def test_concurrent_uuid4_construction() -> None:
    uuideal.install()
    errors: list[BaseException] = []
    values: list[uuid.UUID] = []
    lock = threading.Lock()

    def worker() -> None:
        try:
            local_values = [uuid.uuid4() for _ in range(1000)]
        except BaseException as error:  # test must report thread failures
            errors.append(error)
            return
        with lock:
            values.extend(local_values)

    threads = [threading.Thread(target=worker) for _ in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()

    assert not errors
    assert len(values) == 4000
    assert all(type(value) is uuid.UUID and value.version == 4 for value in values)
    assert len({value.int for value in values}) == len(values)


@pytest.mark.performance
def test_uuid4_performance_smoke_is_faster_than_stdlib() -> None:
    unpatched_time = min(
        timeit.repeat("uuid.uuid4()", globals={"uuid": uuid}, number=100_000, repeat=5)
    )
    uuideal.install()
    patched_time = min(
        timeit.repeat("uuid.uuid4()", globals={"uuid": uuid}, number=100_000, repeat=5)
    )
    assert patched_time < unpatched_time * 0.75
