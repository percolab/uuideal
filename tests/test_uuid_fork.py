from __future__ import annotations

import os
import uuid
from dataclasses import dataclass
from pathlib import Path

import pytest

import uuideal


DEFAULT_FORK_ATTEMPTS = 50
FORK_ATTEMPTS_ENV = "UUIDEA_FORK_ATTEMPTS"


pytestmark = pytest.mark.skipif(
    not hasattr(os, "fork"),
    reason="fork behavior is only defined on platforms with os.fork",
)


@dataclass(frozen=True)
class FirstPostForkUUID:
    attempt: int
    pre_fork_uuid: uuid.UUID
    parent_uuid: uuid.UUID
    child_uuid: uuid.UUID


@dataclass(frozen=True)
class FirstPostForkUUID7:
    attempt: int
    pre_fork_uuid: uuid.UUID
    parent_uuid: uuid.UUID
    child_uuid: uuid.UUID


def fork_attempt_count() -> int:
    raw_attempts = os.environ.get(FORK_ATTEMPTS_ENV)
    if raw_attempts is None:
        return DEFAULT_FORK_ATTEMPTS

    attempts = int(raw_attempts)
    if attempts < 1:
        raise ValueError(f"{FORK_ATTEMPTS_ENV} must be >= 1, got {attempts!r}")
    return attempts


def collect_first_post_fork_uuids(attempt: int, tmp_path: Path) -> FirstPostForkUUID:
    pre_fork_uuid = uuid.uuid4()
    child_uuid_path = tmp_path / f"child-{attempt}.txt"

    pid = os.fork()

    if pid == 0:
        try:
            child_uuid = uuid.uuid4()
            child_uuid_path.write_text(child_uuid.hex, encoding="ascii")
        finally:
            os._exit(0)

    parent_uuid = uuid.uuid4()

    _, status = os.waitpid(pid, 0)
    child_exit_code = os.waitstatus_to_exitcode(status)
    assert child_exit_code == 0

    child_uuid = uuid.UUID(hex=child_uuid_path.read_text(encoding="ascii"))

    return FirstPostForkUUID(
        attempt=attempt,
        pre_fork_uuid=pre_fork_uuid,
        parent_uuid=parent_uuid,
        child_uuid=child_uuid,
    )


def collect_first_post_fork_uuid7(attempt: int, tmp_path: Path) -> FirstPostForkUUID7:
    pre_fork_uuid = uuideal.uuid7()
    child_uuid_path = tmp_path / f"uuid7-child-{attempt}.txt"

    pid = os.fork()

    if pid == 0:
        try:
            child_uuid = uuideal.uuid7()
            child_uuid_path.write_text(child_uuid.hex, encoding="ascii")
        finally:
            os._exit(0)

    parent_uuid = uuideal.uuid7()

    _, status = os.waitpid(pid, 0)
    child_exit_code = os.waitstatus_to_exitcode(status)
    assert child_exit_code == 0

    child_uuid = uuid.UUID(hex=child_uuid_path.read_text(encoding="ascii"))

    return FirstPostForkUUID7(
        attempt=attempt,
        pre_fork_uuid=pre_fork_uuid,
        parent_uuid=parent_uuid,
        child_uuid=child_uuid,
    )


def test_uuid4_first_post_fork_values_are_distinct(tmp_path: Path) -> None:
    uuideal.install()

    for attempt in range(1, fork_attempt_count() + 1):
        result = collect_first_post_fork_uuids(attempt, tmp_path)
        assert result.pre_fork_uuid.version == 4
        assert result.parent_uuid.version == 4
        assert result.child_uuid.version == 4
        assert result.parent_uuid != result.child_uuid, (
            "fork duplicate detected\n"
            f"attempt: {result.attempt}\n"
            f"pre-fork uuid: {result.pre_fork_uuid}\n"
            f"parent first post-fork: {result.parent_uuid.hex}\n"
            f"child first post-fork:  {result.child_uuid.hex}"
        )


def test_uuid7_first_post_fork_values_are_distinct(tmp_path: Path) -> None:
    uuideal.install()

    for attempt in range(1, fork_attempt_count() + 1):
        result = collect_first_post_fork_uuid7(attempt, tmp_path)
        assert result.pre_fork_uuid.version == 7
        assert result.parent_uuid.version == 7
        assert result.child_uuid.version == 7
        assert result.pre_fork_uuid.variant == uuid.RFC_4122
        assert result.parent_uuid.variant == uuid.RFC_4122
        assert result.child_uuid.variant == uuid.RFC_4122
        assert result.parent_uuid != result.child_uuid, (
            "fork duplicate detected\n"
            f"attempt: {result.attempt}\n"
            f"pre-fork uuid7: {result.pre_fork_uuid}\n"
            f"parent first post-fork: {result.parent_uuid.hex}\n"
            f"child first post-fork:  {result.child_uuid.hex}"
        )
