from __future__ import annotations

import uuid

import uuideal


def test_patched_properties_respect_subclass_property_overrides() -> None:
    class MyUUID(uuid.UUID):
        @property
        def hex(self) -> str:
            return "custom-hex"

        @property
        def version(self) -> int:
            return 99

    expected = MyUUID(int=1)

    uuideal.install()

    actual = MyUUID(int=1)
    assert type(actual) is MyUUID
    assert actual.hex == expected.hex == "custom-hex"
    assert actual.version == expected.version == 99
    assert actual.int == expected.int == 1


def test_patched_specials_respect_subclass_method_overrides() -> None:
    class MyUUID(uuid.UUID):
        def __str__(self) -> str:
            return "custom-str"

        def __repr__(self) -> str:
            return "custom-repr"

        def __hash__(self) -> int:
            return 123456

    expected = MyUUID(int=1)

    uuideal.install()

    actual = MyUUID(int=1)
    assert type(actual) is MyUUID
    assert str(actual) == str(expected) == "custom-str"
    assert repr(actual) == repr(expected) == "custom-repr"
    assert hash(actual) == hash(expected) == 123456


def test_patched_uuid_respects_subclass_getattribute_override() -> None:
    class MyUUID(uuid.UUID):
        def __getattribute__(self, name: str):
            if name == "hex":
                return "intercepted-hex"
            return super().__getattribute__(name)

    expected = MyUUID(int=1)

    uuideal.install()

    actual = MyUUID(int=1)
    assert type(actual) is MyUUID
    assert actual.hex == expected.hex == "intercepted-hex"
    assert actual.int == expected.int == 1


def test_patched_uuid_respects_subclass_setattr_override_during_init() -> None:
    class MyUUID(uuid.UUID):
        def __setattr__(self, name: str, value: object) -> None:
            super().__setattr__(name, value)

    expected = MyUUID(int=1)

    uuideal.install()

    actual = MyUUID(int=1)
    assert type(actual) is MyUUID
    assert actual == expected
    assert actual.int == 1


def test_sorting_mixed_uuid_base_and_subclass_matches_stdlib() -> None:
    class MyUUID(uuid.UUID):
        pass

    expected_values = [uuid.UUID(int=2), MyUUID(int=1), uuid.UUID(int=3), MyUUID(int=0)]
    expected = sorted(expected_values)

    uuideal.install()

    actual_values = [uuid.UUID(int=2), MyUUID(int=1), uuid.UUID(int=3), MyUUID(int=0)]
    actual = sorted(actual_values)

    assert actual == expected
    assert [type(value) for value in actual] == [type(value) for value in expected]