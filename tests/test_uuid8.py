from __future__ import annotations

import random
import uuid

import pytest

import uuideal


pytestmark = pytest.mark.skipif(
    not hasattr(uuid, "uuid8"),
    reason="stdlib uuid.uuid8 is not available on this Python version",
)


@pytest.fixture
def restore_random_state() -> object:
    original_state = random.getstate()
    uuideal.uninstall()
    try:
        yield
    finally:
        uuideal.uninstall()
        random.setstate(original_state)


@pytest.mark.parametrize(
    ("a", "b", "c"),
    [
        (None, None, None),
        (0x123456789ABC, None, None),
        (None, 0xABC, None),
        (None, None, 0x123456789ABCDEF),
        (0x123456789ABC, 0xABC, None),
        (0x123456789ABC, None, 0x123456789ABCDEF),
        (None, 0xABC, 0x123456789ABCDEF),
    ],
)
def test_uuid8_omitted_blocks_follow_random_seed(
    restore_random_state: object,
    a: int | None,
    b: int | None,
    c: int | None,
) -> None:
    random.seed(0xA11CE)
    expected = uuid.uuid8(a, b, c)

    uuideal.install()

    random.seed(0xA11CE)
    actual = uuid.uuid8(a, b, c)

    assert actual == expected


def test_uuid8_omitted_blocks_call_random_getrandbits_with_stdlib_widths(
    restore_random_state: object,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[int] = []

    def getrandbits(bits: int) -> int:
        calls.append(bits)
        return {
            48: 0x123456789ABC,
            12: 0xABC,
            62: 0x123456789ABCDEF,
        }[bits]

    monkeypatch.setattr(random, "getrandbits", getrandbits)

    uuideal.install()

    actual = uuid.uuid8(None, None, None)

    assert calls == [48, 12, 62]
    assert actual == uuid.UUID("12345678-9abc-8abc-8123-456789abcdef")