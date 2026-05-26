from __future__ import annotations

import uuid
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Any

import pytest

import uuideal


NODE_A = 0x102030405060
NODE_B = 0xA0B0C0D0E0F0
NODE_C = 0x123456789ABC
NODE_D = 0x0BADF00DCAFE

_UUID1_BYPASSES_GETNODE = getattr(uuid, "_generate_time_safe", None) is not None


@dataclass(frozen=True)
class UUIDImplementation:
    name: str
    module: Any
    installed: bool


@dataclass(frozen=True)
class RecordedOutcome:
    kind: str
    value_node: int | None = None
    error_type: type[BaseException] | None = None
    error_message: str | None = None


def record_uuid1_outcome(call: Callable[[], uuid.UUID]) -> RecordedOutcome:
    try:
        value = call()
    except BaseException as error:
        return RecordedOutcome("error", error_type=type(error), error_message=str(error))
    return RecordedOutcome("value", value_node=value.node)


def assert_same_getnode_outcome(actual: RecordedOutcome, expected: RecordedOutcome) -> None:
    assert actual.kind == expected.kind
    if expected.kind == "error":
        assert actual.error_type is expected.error_type
        assert actual.error_message == expected.error_message
    else:
        assert actual.value_node == expected.value_node


@pytest.fixture
def uuid_implementation(request: pytest.FixtureRequest) -> Iterator[UUIDImplementation]:
    uuideal.uninstall()

    original_getnode = uuid.getnode
    missing = object()
    original_node = getattr(uuid, "_node", missing)

    if request.param == "uuid":
        yield UUIDImplementation("uuid", uuid, installed=False)
    else:
        uuideal.install()
        yield UUIDImplementation("uuideal", uuideal, installed=True)

    uuideal.uninstall()
    uuid.getnode = original_getnode
    if original_node is missing:
        if hasattr(uuid, "_node"):
            delattr(uuid, "_node")
    else:
        uuid._node = original_node


def pytest_generate_tests(metafunc: pytest.Metafunc) -> None:
    if "uuid_implementation" in metafunc.fixturenames:
        metafunc.parametrize(
            "uuid_implementation",
            ["uuid", "uuideal"],
            ids=["uuid", "uuideal"],
            indirect=True,
        )


def next_node(values: Iterator[int], calls: list[int]) -> int:
    value = next(values)
    calls.append(value)
    return value


def get_factory(uuid_implementation: UUIDImplementation, factory_name: str) -> Callable[..., uuid.UUID]:
    if factory_name == "uuid6":
        if uuid_implementation.installed:
            return uuideal.uuid6
        if hasattr(uuid, "uuid6"):
            return uuid.uuid6
        pytest.skip("stdlib uuid.uuid6 is not available on this Python version")

    return getattr(uuid, factory_name)


def assert_uuid1_default_node(expected_node: int) -> None:
    value = uuid.uuid1()
    assert type(value) is uuid.UUID
    assert value.version == 1
    assert value.variant == uuid.RFC_4122
    assert value.node == expected_node


def assert_uuid6_default_node(uuid_implementation: UUIDImplementation, expected_node: int) -> None:
    value = get_factory(uuid_implementation, "uuid6")()
    assert type(value) is uuid.UUID
    assert value.version == 6
    assert value.variant == uuid.RFC_4122
    assert value.node == expected_node


@pytest.mark.skipif(_UUID1_BYPASSES_GETNODE, reason="uuid1() uses _generate_time_safe, never calls getnode")
def test_unknown_getnode_callable_is_not_cached_for_uuid1(uuid_implementation: UUIDImplementation) -> None:
    uuid._node = 0

    calls: list[int] = []
    values = iter((NODE_A, NODE_B))

    def getnode() -> int:
        return next_node(values, calls)

    uuid.getnode = getnode

    assert_uuid1_default_node(NODE_A)
    assert_uuid1_default_node(NODE_B)
    assert calls == [NODE_A, NODE_B]


def test_unknown_getnode_callable_is_not_cached_for_uuid6_shortcut_or_patch(
    uuid_implementation: UUIDImplementation,
) -> None:
    uuid._node = 0

    calls: list[int] = []
    values = iter((NODE_A, NODE_B))

    def getnode() -> int:
        return next_node(values, calls)

    uuid.getnode = getnode

    assert_uuid6_default_node(uuid_implementation, NODE_A)
    assert_uuid6_default_node(uuid_implementation, NODE_B)
    assert calls == [NODE_A, NODE_B]


@pytest.mark.skipif(_UUID1_BYPASSES_GETNODE, reason="uuid1() uses _generate_time_safe, never calls getnode")
def test_node_update_invalidates_trusted_getnode_cache(uuid_implementation: UUIDImplementation) -> None:
    if not uuid_implementation.installed:
        pytest.skip("trusted getnode cache is provided by uuideal")

    uuid._node = NODE_A

    calls: list[int] = []

    def getnode() -> int:
        calls.append(uuid._node)
        return uuid._node

    uuid.getnode = getnode

    assert_uuid1_default_node(NODE_A)
    assert calls == [NODE_A]

    assert_uuid1_default_node(NODE_A)
    assert calls == [NODE_A]

    uuid._node = NODE_B

    assert_uuid1_default_node(NODE_B)
    assert calls == [NODE_A, NODE_B]


def test_node_update_invalidates_trusted_getnode_cache_for_uuid6(
    uuid_implementation: UUIDImplementation,
) -> None:
    if not uuid_implementation.installed:
        pytest.skip("trusted getnode cache is provided by uuideal")

    uuid._node = NODE_A

    calls: list[int] = []

    def getnode() -> int:
        calls.append(uuid._node)
        return uuid._node

    uuid.getnode = getnode

    assert_uuid6_default_node(uuid_implementation, NODE_A)
    assert calls == [NODE_A]

    assert_uuid6_default_node(uuid_implementation, NODE_A)
    assert calls == [NODE_A]

    uuid._node = NODE_B

    assert_uuid6_default_node(uuid_implementation, NODE_B)
    assert calls == [NODE_A, NODE_B]


def test_getnode_change_to_unknown_disables_cache_until_equivalent_callable_is_observed(
    uuid_implementation: UUIDImplementation,
) -> None:
    if not uuid_implementation.installed:
        pytest.skip("trusted getnode cache is provided by uuideal")

    uuid._node = NODE_A

    trusted_calls: list[int] = []

    def trusted_getnode() -> int:
        trusted_calls.append(uuid._node)
        return uuid._node

    uuid.getnode = trusted_getnode
    assert_uuid6_default_node(uuid_implementation, NODE_A)
    assert_uuid6_default_node(uuid_implementation, NODE_A)
    assert trusted_calls == [NODE_A]

    unknown_calls: list[int] = []
    unknown_values = iter((NODE_B, NODE_C))

    def unknown_getnode() -> int:
        return next_node(unknown_values, unknown_calls)

    uuid.getnode = unknown_getnode
    assert_uuid6_default_node(uuid_implementation, NODE_B)
    assert_uuid6_default_node(uuid_implementation, NODE_C)
    assert unknown_calls == [NODE_B, NODE_C]

    uuid._node = NODE_D
    equivalent_calls: list[int] = []

    def equivalent_getnode() -> int:
        equivalent_calls.append(uuid._node)
        return uuid._node

    uuid.getnode = equivalent_getnode
    assert_uuid6_default_node(uuid_implementation, NODE_D)
    assert_uuid6_default_node(uuid_implementation, NODE_D)
    assert equivalent_calls == [NODE_D]


def test_explicit_node_bypasses_getnode_cache_for_uuid1_and_uuid6(
    uuid_implementation: UUIDImplementation,
) -> None:
    uuid._node = NODE_A

    def broken_getnode() -> int:
        raise AssertionError("explicit node must not call uuid.getnode")

    uuid.getnode = broken_getnode

    uuid1_value = uuid.uuid1(node=NODE_B)
    assert type(uuid1_value) is uuid.UUID
    assert uuid1_value.version == 1
    assert uuid1_value.node == NODE_B

    uuid6_factory = get_factory(uuid_implementation, "uuid6")
    uuid6_value = uuid6_factory(node=NODE_C)
    assert type(uuid6_value) is uuid.UUID
    assert uuid6_value.version == 6
    assert uuid6_value.node == NODE_C


@pytest.mark.parametrize("factory_name", ["uuid1", "uuid6"])
def test_explicit_node_none_uses_default_node_cache(
    uuid_implementation: UUIDImplementation,
    factory_name: str,
) -> None:
    if factory_name == "uuid1" and _UUID1_BYPASSES_GETNODE:
        pytest.skip("uuid1() uses _generate_time_safe, never calls getnode")
    if not uuid_implementation.installed:
        pytest.skip("trusted getnode cache is provided by uuideal")

    factory = get_factory(uuid_implementation, factory_name)

    uuid._node = NODE_A

    calls: list[int] = []

    def getnode() -> int:
        calls.append(uuid._node)
        return uuid._node

    uuid.getnode = getnode

    first = factory(node=None)
    second = factory(node=None)

    assert first.node == NODE_A
    assert second.node == NODE_A
    assert calls == [NODE_A]


@pytest.mark.skipif(_UUID1_BYPASSES_GETNODE, reason="uuid1() uses _generate_time_safe, never calls getnode")
def test_deleting_node_invalidates_cache_and_unknown_getnode_still_works(
    uuid_implementation: UUIDImplementation,
) -> None:
    if not uuid_implementation.installed:
        pytest.skip("trusted getnode cache is provided by uuideal")

    uuid._node = NODE_A

    calls: list[int] = []

    def trusted_getnode() -> int:
        calls.append(uuid._node)
        return uuid._node

    uuid.getnode = trusted_getnode
    assert_uuid1_default_node(NODE_A)
    assert_uuid1_default_node(NODE_A)
    assert calls == [NODE_A]

    del uuid._node

    unknown_values = iter((NODE_B, NODE_C))

    def unknown_getnode() -> int:
        return next_node(unknown_values, calls)

    uuid.getnode = unknown_getnode

    assert_uuid1_default_node(NODE_B)
    assert_uuid1_default_node(NODE_C)
    assert calls == [NODE_A, NODE_B, NODE_C]


@pytest.mark.parametrize("factory_name", ["uuid1", "uuid6"])
def test_invalid_default_node_result_matches_unpatched_behavior(factory_name: str) -> None:
    def invalid_getnode() -> int:
        return 1 << 48

    uuideal.uninstall()
    stdlib_factory = get_factory(UUIDImplementation("uuid", uuid, installed=False), factory_name)
    uuid.getnode = invalid_getnode
    expected = record_uuid1_outcome(stdlib_factory)

    uuideal.install()
    patched_factory = get_factory(UUIDImplementation("uuideal", uuideal, installed=True), factory_name)
    uuid.getnode = invalid_getnode
    actual = record_uuid1_outcome(patched_factory)

    assert_same_getnode_outcome(actual, expected)