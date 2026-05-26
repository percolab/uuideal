from __future__ import annotations

import copy
import operator
import pickle
import sys
import uuid
import warnings
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

import pytest

hypothesis = pytest.importorskip("hypothesis")
from hypothesis import HealthCheck, given, settings, strategies as st

import uuideal
from test_uuid_patch import INVALID_UUID_CALLS, snapshot


NODE_A = 0x102030405060


FUZZ_SETTINGS = settings(
    max_examples=500,
    suppress_health_check=(HealthCheck.too_slow,),
    deadline=None,
)


WarningSnapshot = tuple[type[Warning], str]


@dataclass(frozen=True)
class RecordedOutcome:
    kind: str
    value: Any = None
    error_type: type[BaseException] | None = None
    error_message: str | None = None
    warnings: tuple[WarningSnapshot, ...] = ()

    def describe(self) -> str:
        warning_text = (
            ""
            if not self.warnings
            else f", warnings={[(category.__name__, message) for category, message in self.warnings]!r}"
        )
        if self.kind == "error":
            error_name = self.error_type.__name__ if self.error_type is not None else None
            return f"error(type={error_name!r}, message={self.error_message!r}{warning_text})"
        return f"value({self.value!r}{warning_text})"


def record_outcome(call: Callable[[], Any], meta: Any = None) -> RecordedOutcome:
    with warnings.catch_warnings(record=True) as caught_warnings:
        warnings.simplefilter("always")
        try:
            value = call()
        except BaseException as error:
            warning_snapshots = tuple(
                (warning.category, str(warning.message)) for warning in caught_warnings
            )
            return RecordedOutcome(
                "error",
                error_type=type(error),
                error_message=str(error),
                warnings=warning_snapshots,
            )

    warning_snapshots = tuple((warning.category, str(warning.message)) for warning in caught_warnings)
    return RecordedOutcome("value", value, warnings=warning_snapshots)


def assert_same_outcome(actual: RecordedOutcome, expected: RecordedOutcome, context: str = "") -> None:
    assert actual.kind == expected.kind, (
        f"{context}\nexpected: {expected.describe()}\nactual:   {actual.describe()}"
    )
    assert actual.warnings == expected.warnings, (
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


def record_uuid_observation(call: Callable[[], uuid.UUID]) -> RecordedOutcome:
    return record_outcome(lambda: uuid_observation(call()))


def generated_uuid_summary(value: uuid.UUID) -> dict[str, Any]:
    return {
        "type": type(value).__name__,
        "version": value.version,
        "variant": value.variant,
        "node": value.node,
        # "clock_seq": value.clock_seq,
        "is_safe": value.is_safe,
    }


def record_generated_uuid_summary(call: Callable[[], uuid.UUID]) -> RecordedOutcome:
    return record_outcome(lambda: generated_uuid_summary(call()))


def assert_same_uuid_observation(
    call: Callable[[], uuid.UUID],
    expected: RecordedOutcome,
    context: str = "",
) -> None:
    actual = record_uuid_observation(call)
    assert_same_outcome(actual, expected, context)


def constructor_case_strategy() -> st.SearchStrategy[tuple[tuple[Any, ...], dict[str, Any]]]:
    int_values = st.integers(min_value=0, max_value=(1 << 128) - 1)
    raw_16 = st.binary(min_size=16, max_size=16)
    text = st.one_of(
        int_values.map(lambda value: f"{value:032x}"),
        int_values.map(lambda value: str(uuid.UUID(int=value))),
        int_values.map(lambda value: "{" + str(uuid.UUID(int=value)) + "}"),
        int_values.map(lambda value: "urn:uuid:" + str(uuid.UUID(int=value))),
    )
    fields = st.tuples(
        st.integers(min_value=0, max_value=(1 << 32) - 1),
        st.integers(min_value=0, max_value=(1 << 16) - 1),
        st.integers(min_value=0, max_value=(1 << 16) - 1),
        st.integers(min_value=0, max_value=(1 << 8) - 1),
        st.integers(min_value=0, max_value=(1 << 8) - 1),
        st.integers(min_value=0, max_value=(1 << 48) - 1),
    )
    version_values = st.one_of(st.none(), st.integers(min_value=1, max_value=8))
    is_safe_values = st.sampled_from(
        [uuid.SafeUUID.safe, uuid.SafeUUID.unsafe, uuid.SafeUUID.unknown]
    )

    source = st.one_of(
        text.map(lambda value: ((), {"hex": value})),
        text.map(lambda value: ((value,), {})),
        raw_16.map(lambda value: ((), {"bytes": value})),
        raw_16.map(lambda value: ((), {"bytes_le": value})),
        int_values.map(lambda value: ((), {"int": value})),
        fields.map(lambda value: ((), {"fields": value})),
    )

    return st.builds(
        lambda source_case, version, is_safe: (
            source_case[0],
            {
                **source_case[1],
                **({} if version is None else {"version": version}),
                "is_safe": is_safe,
            },
        ),
        source,
        version_values,
        is_safe_values,
    )


def invalid_constructor_case_strategy() -> st.SearchStrategy[tuple[tuple[Any, ...], dict[str, Any]]]:
    bad_hex = st.one_of(
        st.text(min_size=0, max_size=80).filter(
            lambda value: record_outcome(lambda: uuid.UUID(value)).kind == "error"
        ),
        st.binary(min_size=0, max_size=80),
    )
    bad_bytes = st.binary(min_size=0, max_size=40).filter(lambda value: len(value) != 16)
    bad_int = st.one_of(
        st.integers(max_value=-1),
        st.integers(min_value=1 << 128, max_value=(1 << 140)),
    )
    bad_fields = st.one_of(
        st.lists(st.integers(min_value=0, max_value=10), min_size=0, max_size=5).map(tuple),
        st.lists(st.integers(min_value=0, max_value=10), min_size=7, max_size=10).map(tuple),
        st.tuples(
            st.integers(min_value=1 << 32, max_value=1 << 40),
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
        ),
        st.tuples(
            st.just(0),
            st.integers(min_value=1 << 16, max_value=1 << 24),
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
        ),
        st.tuples(
            st.just(0),
            st.just(0),
            st.integers(min_value=1 << 16, max_value=1 << 24),
            st.just(0),
            st.just(0),
            st.just(0),
        ),
        st.tuples(
            st.just(0),
            st.just(0),
            st.just(0),
            st.integers(min_value=1 << 8, max_value=1 << 16),
            st.just(0),
            st.just(0),
        ),
        st.tuples(
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
            st.integers(min_value=1 << 8, max_value=1 << 16),
            st.just(0),
        ),
        st.tuples(
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
            st.just(0),
            st.integers(min_value=1 << 48, max_value=1 << 60),
        ),
    )
    bad_version = st.one_of(
        st.integers(max_value=-10),
        st.integers(min_value=9, max_value=100),
        st.text(min_size=0, max_size=5),
    )

    return st.one_of(
        bad_hex.map(lambda value: ((value,), {})),
        bad_hex.map(lambda value: ((), {"hex": value})),
        bad_bytes.map(lambda value: ((), {"bytes": value})),
        bad_bytes.map(lambda value: ((), {"bytes_le": value})),
        bad_int.map(lambda value: ((), {"int": value})),
        bad_fields.map(lambda value: ((), {"fields": value})),
        bad_version.map(lambda value: ((), {"int": 0, "version": value})),
        st.just((("12345678123456781234567812345678",), {"int": 0})),
        st.just(((), {})),
    )


@FUZZ_SETTINGS
@given(constructor_case_strategy())
def test_fuzz_uuid_constructor_valid_inputs_match_stdlib(
    case: tuple[tuple[Any, ...], dict[str, Any]],
) -> None:
    constructor_args, constructor_kwargs = case
    if constructor_kwargs.get("version") in {6, 7, 8} and not hasattr(uuid, "uuid6"):
        pytest.skip("stdlib uuid only accepts versions 6, 7, and 8 on newer Python versions")

    expected = record_uuid_observation(lambda: uuid.UUID(*constructor_args, **constructor_kwargs))

    uuideal.install()

    assert_same_uuid_observation(lambda: uuid.UUID(*constructor_args, **constructor_kwargs), expected)


@FUZZ_SETTINGS
@given(invalid_constructor_case_strategy())
def test_fuzz_uuid_constructor_invalid_inputs_match_stdlib(
    case: tuple[tuple[Any, ...], dict[str, Any]],
) -> None:
    constructor_args, constructor_kwargs = case
    expected = record_outcome(lambda: uuid.UUID(*constructor_args, **constructor_kwargs))
    assert expected.kind == "error"

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID(*constructor_args, **constructor_kwargs))
    assert_same_outcome(actual, expected)


HASH_FACTORY_NAMES = st.sampled_from(["uuid3", "uuid5"])
NAMES = st.one_of(
    st.text(max_size=128),
    st.binary(max_size=128),
)
NAMESPACES = st.sampled_from(
    [uuid.NAMESPACE_DNS, uuid.NAMESPACE_URL, uuid.NAMESPACE_OID, uuid.NAMESPACE_X500]
)


@FUZZ_SETTINGS
@given(HASH_FACTORY_NAMES, NAMESPACES, NAMES)
def test_fuzz_deterministic_factories_match_stdlib(
    factory_name: str,
    namespace: uuid.UUID,
    name: str | bytes,
) -> None:
    factory = getattr(uuid, factory_name)
    expected = record_uuid_observation(lambda: factory(namespace, name))

    uuideal.install()

    assert_same_uuid_observation(lambda: factory(namespace, name), expected)


@FUZZ_SETTINGS
@given(HASH_FACTORY_NAMES, st.one_of(st.none(), st.integers(), st.text()), NAMES)
def test_fuzz_deterministic_factory_invalid_namespace_matches_stdlib(
    factory_name: str,
    namespace: object,
    name: str | bytes,
) -> None:
    factory = getattr(uuid, factory_name)
    expected = record_outcome(lambda: factory(namespace, name), (namespace, name))
    assert expected.kind == "error"

    uuideal.install()

    actual = record_outcome(lambda: factory(namespace, name))
    assert_same_outcome(actual, expected)


@FUZZ_SETTINGS
@given(st.integers(min_value=0, max_value=(1 << 128) - 1))
def test_fuzz_uuid_properties_and_methods_match_stdlib(value: int) -> None:
    expected_uuid = uuid.UUID(int=value)
    expected_observation = uuid_observation(expected_uuid)
    expected_dir = sorted(dir(expected_uuid))

    uuideal.install()

    actual_uuid = uuid.UUID(int=value)
    assert uuid_observation(actual_uuid) == expected_observation
    assert sorted(dir(actual_uuid)) == expected_dir


@FUZZ_SETTINGS
@given(
    st.integers(min_value=0, max_value=(1 << 128) - 1),
    st.integers(min_value=0, max_value=(1 << 128) - 1),
)
def test_fuzz_uuid_comparisons_match_stdlib(left_value: int, right_value: int) -> None:
    left = uuid.UUID(int=left_value)
    right = uuid.UUID(int=right_value)
    operations = (operator.eq, operator.lt, operator.gt, operator.le, operator.ge)
    expected = {
        operation.__name__: record_outcome(lambda operation=operation: operation(left, right))
        for operation in operations
    }
    expected_not_implemented = uuid.UUID.__eq__(left, object())

    uuideal.install()

    actual_left = uuid.UUID(int=left_value)
    actual_right = uuid.UUID(int=right_value)
    actual = {
        operation.__name__: record_outcome(
            lambda operation=operation: operation(actual_left, actual_right)
        )
        for operation in operations
    }
    assert actual == expected
    assert uuid.UUID.__eq__(actual_left, object()) is expected_not_implemented


@FUZZ_SETTINGS
@given(st.one_of(st.integers(min_value=0, max_value=(1 << 128) - 1), st.booleans()))
def test_fuzz_from_int_matches_stdlib(value: int | bool) -> None:
    if not hasattr(uuid.UUID, "_from_int"):
        pytest.skip("uuid.UUID._from_int is not available on this Python version")

    class FuzzUUID(uuid.UUID):
        pass

    expected_uuid = record_uuid_observation(lambda: uuid.UUID._from_int(value))
    expected_subclass = record_uuid_observation(lambda: FuzzUUID._from_int(value))

    uuideal.install()

    assert_same_uuid_observation(lambda: uuid.UUID._from_int(value), expected_uuid)
    assert_same_uuid_observation(lambda: FuzzUUID._from_int(value), expected_subclass)


@FUZZ_SETTINGS
@given(
    st.one_of(
        st.integers(max_value=-1),
        st.integers(min_value=1 << 128, max_value=1 << 140),
        st.text(max_size=16),
        st.binary(max_size=16),
    )
)
def test_fuzz_from_int_invalid_inputs_match_stdlib(value: object) -> None:
    if not hasattr(uuid.UUID, "_from_int"):
        pytest.skip("uuid.UUID._from_int is not available on this Python version")

    expected = record_outcome(lambda: uuid.UUID._from_int(value))
    assert expected.kind == "error"

    uuideal.install()

    actual = record_outcome(lambda: uuid.UUID._from_int(value))
    assert_same_outcome(actual, expected)


@FUZZ_SETTINGS
@given(st.integers(min_value=0, max_value=(1 << 48) - 1), st.integers(min_value=0, max_value=0x3FFF))
def test_fuzz_uuid1_explicit_inputs_static_fields_match_stdlib(node: int, clock_seq: int) -> None:
    expected = uuid.uuid1(node=node, clock_seq=clock_seq)

    uuideal.install()

    actual = uuid.uuid1(node=node, clock_seq=clock_seq)
    assert type(actual) is type(expected)
    assert actual.version == expected.version
    assert actual.variant == expected.variant
    assert actual.node == expected.node
    assert actual.clock_seq == expected.clock_seq
    assert actual.is_safe == expected.is_safe


@FUZZ_SETTINGS
@given(
    st.one_of(
        st.integers(max_value=-1),
        st.integers(min_value=1 << 48, max_value=1 << 64),
        st.text(max_size=8),
        st.binary(max_size=8),
    ),
    st.one_of(st.none(), st.integers(min_value=0, max_value=0x3FFF)),
)
def test_fuzz_uuid1_invalid_node_matches_stdlib(node: object, clock_seq: object) -> None:
    expected = record_outcome(lambda: uuid.uuid1(node=node, clock_seq=clock_seq))
    assert expected.kind == "error"

    uuideal.install()

    actual = record_outcome(lambda: uuid.uuid1(node=node, clock_seq=clock_seq))
    assert_same_outcome(actual, expected)


@FUZZ_SETTINGS
@given(st.integers(min_value=0, max_value=(1 << 128) - 1))
def test_fuzz_getstate_setstate_roundtrip_matches_stdlib(value: int) -> None:
    original = uuid.UUID(int=value)
    expected_state = original.__getstate__()
    expected_restored = object.__new__(uuid.UUID)
    expected_restored.__setstate__(expected_state)

    uuideal.install()

    actual = uuid.UUID(int=value)
    actual_state = actual.__getstate__()
    actual_restored = object.__new__(uuid.UUID)
    actual_restored.__setstate__(actual_state)

    assert actual_state == expected_state
    assert actual_restored == expected_restored
    assert actual_restored.is_safe is expected_restored.is_safe


@FUZZ_SETTINGS
@given(st.sampled_from(INVALID_UUID_CALLS))
def test_fuzz_representative_invalid_calls_match_stdlib(call: object) -> None:
    expected = record_outcome(call.values[0])
    assert expected.kind == "error"

    uuideal.install()

    actual = record_outcome(call.values[0])
    assert_same_outcome(actual, expected)


@FUZZ_SETTINGS
@given(
    st.one_of(st.none(), st.integers(min_value=0, max_value=(1 << 64) - 1)),
    st.one_of(st.none(), st.integers(min_value=0, max_value=(1 << 64) - 1)),
    st.one_of(st.none(), st.integers(min_value=0, max_value=(1 << 80) - 1)),
)
def test_fuzz_uuid8_custom_blocks_match_stdlib_static_fields(
    a: int | None,
    b: int | None,
    c: int | None,
) -> None:
    if not hasattr(uuid, "uuid8"):
        pytest.skip("stdlib uuid.uuid8 is not available on this Python version")

    expected = uuid.uuid8(a, b, c)

    uuideal.install()

    actual = uuid.uuid8(a, b, c)
    assert type(actual) is type(expected)
    assert actual.version == expected.version
    assert actual.variant == expected.variant
    if a is not None:
        assert (actual.int >> 80) == (expected.int >> 80)
    if b is not None:
        assert ((actual.int >> 64) & 0x0FFF) == ((expected.int >> 64) & 0x0FFF)
    if c is not None:
        assert (actual.int & 0x3FFF_FFFF_FFFF_FFFF) == (
            expected.int & 0x3FFF_FFFF_FFFF_FFFF
        )


@FUZZ_SETTINGS
@given(
    st.sampled_from(["uuid1", "uuid6"]),
    st.one_of(
        st.integers(max_value=-1),
        st.integers(min_value=1 << 48, max_value=1 << 64),
        st.text(max_size=16),
        st.binary(max_size=16),
        st.none(),
    ),
)
def test_fuzz_getnode_behavior_matches_unpatched_stdlib(
    factory_name: str,
    node_result: object,
) -> None:
    if factory_name == "uuid6" and not hasattr(uuid, "uuid6"):
        pytest.skip("stdlib uuid.uuid6 is not available on this Python version")

    factory = getattr(uuid, factory_name)
    original_getnode = uuid.getnode
    missing = object()
    original_node = getattr(uuid, "_node", missing)

    def getnode() -> object:
        return node_result

    context = (
        f"factory_name={factory_name!r}, "
        f"node_result={node_result!r}, "
        f"node_result_type={type(node_result).__name__}"
    )

    try:
        uuid._node = NODE_A
        uuid.getnode = getnode
        expected = record_generated_uuid_summary(factory)

        uuideal.install()
        uuid._node = NODE_A
        uuid.getnode = getnode
        actual = record_generated_uuid_summary(factory)

        assert_same_outcome(actual, expected, context)
    finally:
        uuideal.uninstall()
        uuid.getnode = original_getnode
        if original_node is missing:
            if hasattr(uuid, "_node"):
                delattr(uuid, "_node")
        else:
            uuid._node = original_node