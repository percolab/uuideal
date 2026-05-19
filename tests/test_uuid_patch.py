from __future__ import annotations

import copy
import pickle
import sys
import threading
import timeit
import uuid

import pytest

import uuideal


@pytest.fixture(autouse=True)
def clean_patch_state():
    uuideal.uninstall()
    yield
    uuideal.uninstall()


def assert_uuid_equivalent(left: uuid.UUID, right: uuid.UUID) -> None:
    assert type(right) is uuid.UUID
    for attribute_name in (
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
    ):
        assert getattr(right, attribute_name) == getattr(left, attribute_name), attribute_name
    assert repr(right) == repr(left)
    assert str(right) == str(left)
    assert hash(right) == hash(left)
    assert right == left
    assert int(right) == int(left)
    assert bool(right) == bool(left)
    assert dir(right) == dir(left)
    assert sys.getsizeof(right) == sys.getsizeof(left)


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
    assert uuideal.is_enabled()

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
    assert not uuideal.is_enabled()


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
    uuideal.install()
    actual = uuid.UUID(*constructor_args, **constructor_kwargs)
    assert_uuid_equivalent(expected, actual)


def test_patched_properties_work_on_unpatched_objects() -> None:
    stdlib_object = uuid.UUID("12345678-1234-5678-9234-567812345678")
    expected_values = {name: getattr(stdlib_object, name) for name in (
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
    )}

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
    uuideal.install()
    actual = factory(namespace, name)
    assert_uuid_equivalent(expected, actual)
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

def test_error_equivalence_for_representative_invalid_inputs() -> None:
    invalid_calls = [
        lambda: uuid.UUID(),
        lambda: uuid.UUID("bad"),
        lambda: uuid.UUID(bytes=b"short"),
        lambda: uuid.UUID(int=1 << 128),
        lambda: uuid.UUID(fields=(1, 2, 3)),
        lambda: uuid.UUID("12345678123456781234567812345678", hex="12345678123456781234567812345678"),
        lambda: uuid.uuid4(1),
        lambda: uuid.uuid3(uuid.NAMESPACE_DNS),
    ]
    expected_errors: list[tuple[type[BaseException], str]] = []
    for call in invalid_calls:
        with pytest.raises(Exception) as captured:
            call()
        expected_errors.append((type(captured.value), str(captured.value)))

    uuideal.install()

    for call, (expected_type, expected_message) in zip(invalid_calls, expected_errors, strict=True):
        with pytest.raises(expected_type) as captured:
            call()
        assert str(captured.value) == expected_message


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


def test_uuid4_performance_smoke_is_faster_than_stdlib() -> None:
    unpatched_time = timeit.timeit("uuid.uuid4()", globals={"uuid": uuid}, number=100_000)
    uuideal.install()
    patched_time = timeit.timeit("uuid.uuid4()", globals={"uuid": uuid}, number=100_000)
    assert patched_time < unpatched_time
